#!/usr/bin/env bash
# qi 开发用的常驻测试库（PostgreSQL + MySQL）
#
# ── 为什么要常驻 ────────────────────────────────────────────────
#
# 多后端数据库（qi/src/runtime/stdlib/database/）没法只靠单元测试验：
# 占位符改写、类型降级、事务语义这些，只有连真库才知道对不对。
# 每次验证临时起容器要等镜像和初始化，且容易忘了清理；常驻两个小容器
# （加起来约 500MB 磁盘、闲时几乎不占 CPU）省掉这份摩擦。
#
# 只绑 127.0.0.1 —— 密码是写死的开发口令，绝不能让它露到局域网。
# 数据放命名卷，容器删了重建数据还在；机器重启后 Docker 会自己把它们拉起来。
#
# 用法：
#   ./测试库.sh 起      建/启动两个库（幂等，已在跑就跳过）
#   ./测试库.sh 停      停掉（数据保留）
#   ./测试库.sh 状态    看是否就绪 + 连接串
#   ./测试库.sh 删      连数据一起删（要重来时用）
set -euo pipefail

# shell 标识符一律 ASCII —— bash 的变量名不接受非 ASCII（报「未找到命令」，
# 一眼看不出是名字的问题）。中文只写在 qi 里。
PG_NAME=qi-dev-pg
MY_NAME=qi-dev-mysql
PG_PORT=45432          # 避开 5432：本机常有别的项目占着
MY_PORT=43306          # 同理避开 3306
USER=qi
PASS=qidev
DB=qi_dev

PG_URL="postgres://$USER:$PASS@127.0.0.1:$PG_PORT/$DB"
MY_URL="mysql://$USER:$PASS@127.0.0.1:$MY_PORT/$DB"

起() {
    docker volume create qi-dev-pg-data >/dev/null
    docker volume create qi-dev-mysql-data >/dev/null

    if [ -n "$(docker ps -q -f name="^${PG_NAME}$")" ]; then
        echo "· $PG_NAME 已在跑"
    elif [ -n "$(docker ps -aq -f name="^${PG_NAME}$")" ]; then
        docker start "$PG_NAME" >/dev/null && echo "· $PG_NAME 已启动"
    else
        docker run -d --name "$PG_NAME" --restart unless-stopped \
            -p "127.0.0.1:$PG_PORT:5432" \
            -e POSTGRES_USER=$USER -e POSTGRES_PASSWORD=$PASS -e POSTGRES_DB=$DB \
            -v qi-dev-pg-data:/var/lib/postgresql/data \
            postgres:17-alpine >/dev/null && echo "· $PG_NAME 已创建"
    fi

    if [ -n "$(docker ps -q -f name="^${MY_NAME}$")" ]; then
        echo "· $MY_NAME 已在跑"
    elif [ -n "$(docker ps -aq -f name="^${MY_NAME}$")" ]; then
        docker start "$MY_NAME" >/dev/null && echo "· $MY_NAME 已启动"
    else
        docker run -d --name "$MY_NAME" --restart unless-stopped \
            -p "127.0.0.1:$MY_PORT:3306" \
            -e MYSQL_ROOT_PASSWORD=$PASS -e MYSQL_DATABASE=$DB \
            -e MYSQL_USER=$USER -e MYSQL_PASSWORD=$PASS \
            -v qi-dev-mysql-data:/var/lib/mysql \
            mysql:8.4 >/dev/null && echo "· $MY_NAME 已创建"
    fi

    等就绪
    状态
}

等就绪() {
    printf '· 等待就绪'
    for _ in $(seq 1 40); do
        if docker exec "$PG_NAME" pg_isready -U $USER -d $DB >/dev/null 2>&1 &&
           docker exec "$MY_NAME" mysqladmin ping -u$USER -p$PASS >/dev/null 2>&1; then
            echo " ✓"
            return 0
        fi
        printf '.'
        sleep 3
    done
    echo " ✗ 超时，看 docker logs $PG_NAME / $MY_NAME"
    return 1
}

状态() {
    echo
    for one in "$PG_NAME" "$MY_NAME"; do
        printf '%-16s %s\n' "$one" "$(docker ps -a --filter "name=^${one}$" --format '{{.Status}}' || echo 未创建)"
    done
    echo
    echo "连接串（跑验证时用）："
    echo "  QI_DB_URL=\"$PG_URL\""
    echo "  QI_DB_URL=\"$MY_URL\""
}

停() { docker stop "$PG_NAME" "$MY_NAME" >/dev/null 2>&1 || true; echo "已停（数据保留在命名卷里）"; }

删() {
    docker rm -f "$PG_NAME" "$MY_NAME" >/dev/null 2>&1 || true
    docker volume rm qi-dev-pg-data qi-dev-mysql-data >/dev/null 2>&1 || true
    echo "容器与数据卷都已删除"
}

case "${1:-起}" in
    起|start|up) 起 ;;
    停|stop|down) 停 ;;
    状态|status) 状态 ;;
    删|rm|clean) 删 ;;
    *) echo "用法: $0 [起|停|状态|删]"; exit 1 ;;
esac
