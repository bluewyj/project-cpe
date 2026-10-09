#!/bin/sh
# 每分钟监测 ofono 的网络制式 (NR/LTE)
#
# 约束：
# 1. 固定 LTE only：绝不改写 TechnologyPreference
# 2. 固定 NR 5G only：保留原版「强制 NR」促切逻辑；促切后写回 NR 5G only
# 3. 自动模式：未确认 NR 能力时不促切（避免 4G 卡被周期性强切）
# 4. 自动模式且已确认 NR：对「卡在 LTE」做原版促切（结束后保持 auto）
#
# 阶段一：检测到 LTE 时，最多尝试切换 3 次
# 阶段二：如果 3 次切换后仍然是 LTE，则进入冷却期，冷却期结束后只允许 1 次切换
# 冷却期逐步延长：10 分钟 → 30 分钟 → 60 分钟（不再延长）
# 如果切换后连续两次检测到 NR，则恢复到阶段一逻辑
# 日志写入 /var/volatile/nr_lte_switch.log，最多保存 120 条

LOGDIR=/var/volatile
LOGFILE=$LOGDIR/nr_lte_switch.log
# 持久标记：曾观察到 NR / 判定无 NR（仅约束自动模式）
NR_CAPABLE_FILE=/mnt/data/nr_lte_switch.nr_capable
NO_NR_FILE=/mnt/data/nr_lte_switch.no_nr
mkdir -p "$LOGDIR"

phase=1               # 当前阶段：1=三次尝试阶段，2=冷却期阶段
switch_count=0        # 阶段一已执行的切换次数
cooldown=0            # 当前冷却期长度（分钟）
cooldown_counter=0    # 冷却期剩余检测次数
nr_streak=0           # 连续检测到 NR 的次数
last_switch=0         # 冷却期阶段是否刚执行过一次切换

log_line() {
    TS=$(date '+%Y-%m-%d %H:%M:%S')
    echo "$TS - $*" >> "$LOGFILE"
    tail -n 120 "$LOGFILE" > "$LOGFILE.tmp" && mv -f "$LOGFILE.tmp" "$LOGFILE"
}

get_radio_props() {
    dbus-send --system --print-reply \
        --dest=org.ofono /ril_0 org.ofono.RadioSettings.GetProperties 2>/dev/null
}

get_pref() {
    # dbus 行形如: variant             string "LTE only"
    # 必须按引号取值，避免 awk 字段把 "LTE only" 截成 LTE
    echo "$1" | grep -A1 'string "TechnologyPreference"' | tail -n1 | awk -F'"' '{print $2}'
}

set_pref() {
    dbus-send --system --print-reply \
      --dest=org.ofono /ril_0 org.ofono.RadioSettings.SetProperty \
      string:"TechnologyPreference" variant:string:"$1" >/dev/null 2>&1
}

is_lte_only() {
    [ "$1" = "LTE only" ]
}

is_nr_only() {
    [ "$1" = "NR 5G only" ]
}

is_auto_pref() {
    case "$1" in
        "NR 5G/LTE auto"|"LTE/GSM/WCDMA auto"|"NR 5G/LTE/GSM/WCDMA auto"|"") return 0 ;;
        *) return 1 ;;
    esac
}

mark_nr_capable() {
    rm -f "$NO_NR_FILE" 2>/dev/null
    touch "$NR_CAPABLE_FILE" 2>/dev/null
}

mark_no_nr() {
    rm -f "$NR_CAPABLE_FILE" 2>/dev/null
    touch "$NO_NR_FILE" 2>/dev/null
}

# 是否确认具备 NR：曾驻网 NR / AvailableTechnologies 含 NR|5G / 持久标记
has_nr_capability() {
    [ -f "$NR_CAPABLE_FILE" ] && return 0
    [ -f "$NO_NR_FILE" ] && return 1
    [ "$1" = "NR" ] && { mark_nr_capable; return 0; }
    echo "$2" | grep -Eiq 'string "([^"]*NR[^"]*|[^"]*5G[^"]*)"' && { mark_nr_capable; return 0; }
    return 1
}

# 原版强制 NR：NR only → auto；若调用方要求固定 5G，再写回 NR only
# $1 = restore 目标：空=保持 auto；"NR 5G only"=促切后恢复固定 5G
do_promote_switch() {
    set_pref "NR 5G only"
    set_pref "NR 5G/LTE auto"
    if [ "$1" = "NR 5G only" ]; then
        set_pref "NR 5G only"
    fi
}

reset_phase_state() {
    phase=1
    switch_count=0
    cooldown=0
    cooldown_counter=0
    nr_streak=0
    last_switch=0
}

# 对「卡在 LTE」执行原版阶段机。$1=restore pref（空或 NR 5G only）
# $2=是否允许因失败标记 no_nr（auto=1，固定5G=0）
handle_lte_promote() {
    restore="$1"
    allow_mark_no_nr="$2"
    nr_streak=0

    if [ $phase -eq 1 ]; then
        if [ $switch_count -lt 3 ]; then
            do_promote_switch "$restore"
            switch_count=$((switch_count+1))
            LOGMSG="阶段一：检测到 LTE，执行强制 NR 切换（第 $switch_count 次）"
        else
            phase=2
            cooldown=10
            cooldown_counter=$cooldown
            last_switch=0
            if [ "$allow_mark_no_nr" = "1" ]; then
                mark_no_nr
                LOGMSG="阶段一：已切换 3 次仍为 LTE，标记无 NR 并进入冷却期 $cooldown 分钟"
            else
                LOGMSG="阶段一：已切换 3 次仍为 LTE，进入冷却期 $cooldown 分钟（固定5G，不标记无 NR）"
            fi
        fi
        return
    fi

    # 阶段二
    if [ $cooldown_counter -gt 0 ]; then
        cooldown_counter=$((cooldown_counter-1))
        if [ $cooldown_counter -eq 0 ]; then
            LOGMSG="阶段二：检测到 LTE，冷却期结束，允许一次切换"
            last_switch=0
        else
            LOGMSG="阶段二：检测到 LTE，处于冷却期（剩余 $cooldown_counter 次检测）"
        fi
        return
    fi

    if [ $last_switch -eq 0 ]; then
        if [ "$allow_mark_no_nr" = "1" ] && [ -f "$NO_NR_FILE" ]; then
            LOGMSG="阶段二：已标记无 NR，跳过冷却期切换"
            last_switch=1
        else
            do_promote_switch "$restore"
            last_switch=1
            LOGMSG="阶段二：检测到 LTE，执行强制 NR 冷却期切换"
        fi
        return
    fi

    case $cooldown in
        10) cooldown=30 ;;
        30) cooldown=60 ;;
        60) cooldown=60 ;;
    esac
    cooldown_counter=$cooldown
    last_switch=0
    if [ "$allow_mark_no_nr" = "1" ]; then
        mark_no_nr
        LOGMSG="阶段二：检测到 LTE，切换后仍为 LTE，标记无 NR，进入冷却期 ${cooldown} 分钟"
    else
        LOGMSG="阶段二：检测到 LTE，切换后仍为 LTE，进入冷却期 ${cooldown} 分钟（固定5G继续重试）"
    fi
}

while true; do
    RADIO_PROPS=$(get_radio_props)
    PREF=$(get_pref "$RADIO_PROPS")
    [ -z "$PREF" ] && PREF="null"

    PROPS=$(dbus-send --system --print-reply \
        --dest=org.ofono /ril_0 org.ofono.NetworkRegistration.GetProperties 2>/dev/null)

    TECH=$(echo "$PROPS" | grep -A1 'string "Technology"' | tail -n1 | awk '{print $3}' | tr -d '"')
    STRENGTH=$(echo "$PROPS" | grep -A1 'string "StrengthDbm"' | tail -n1 | awk '{print $3}' | tr -d '"')

    [ -z "$TECH" ] && TECH="null"
    [ -z "$STRENGTH" ] && STRENGTH="null"

    if [ "$TECH" = "null" ]; then
        log_line "当前制式: $TECH, 信号强度(dBm): $STRENGTH, 偏好: $PREF —— 未获取到网络信息，跳过本次检测"
        sleep 60
        continue
    fi

    if [ "$TECH" = "NR" ]; then
        mark_nr_capable
    fi

    # 1) 固定 4G：绝不改写
    if is_lte_only "$PREF"; then
        log_line "当前制式: $TECH, 信号强度(dBm): $STRENGTH, 偏好: $PREF —— 固定 LTE，不改写"
        reset_phase_state
        sleep 60
        continue
    fi

    # 2) 固定 5G：启用原版强制 NR（促切后写回 NR 5G only）
    if is_nr_only "$PREF"; then
        if [ "$TECH" = "NR" ]; then
            nr_streak=$((nr_streak+1))
            if [ $nr_streak -ge 2 ]; then
                reset_phase_state
                LOGMSG="固定5G：检测到 NR（连续两次），恢复到阶段一逻辑"
            else
                LOGMSG="固定5G：已在 NR，保持不切换"
            fi
        elif [ "$TECH" = "LTE" ]; then
            handle_lte_promote "NR 5G only" "0"
        else
            LOGMSG="固定5G：检测到未知状态: $TECH"
        fi
        log_line "当前制式: $TECH, 信号强度(dBm): $STRENGTH, 偏好: $PREF —— $LOGMSG"
        sleep 60
        continue
    fi

    # 3) 非自动偏好：保守不改
    if ! is_auto_pref "$PREF"; then
        log_line "当前制式: $TECH, 信号强度(dBm): $STRENGTH, 偏好: $PREF —— 非自动偏好，不改写"
        sleep 60
        continue
    fi

    # 4) 自动模式但未确认 NR：不促切
    if ! has_nr_capability "$TECH" "$RADIO_PROPS"; then
        log_line "当前制式: $TECH, 信号强度(dBm): $STRENGTH, 偏好: $PREF —— 未确认 NR 能力（4G 卡或不促切），跳过"
        sleep 60
        continue
    fi

    # 5) 自动 + 有 NR：原版促切（结束后保持 auto）
    if [ "$TECH" = "NR" ]; then
        nr_streak=$((nr_streak+1))
        if [ $nr_streak -ge 2 ]; then
            reset_phase_state
            LOGMSG="检测到 NR（连续两次），恢复到阶段一逻辑"
        else
            LOGMSG="检测到 NR，保持不切换"
        fi
    elif [ "$TECH" = "LTE" ]; then
        handle_lte_promote "" "1"
    else
        LOGMSG="检测到未知状态: $TECH"
    fi

    log_line "当前制式: $TECH, 信号强度(dBm): $STRENGTH, 偏好: $PREF —— $LOGMSG"
    sleep 60
done
