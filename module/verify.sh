#!/system/bin/sh
# 校验清单的核对逻辑，安装时由 customize.sh 解出来再 source。
#
# 为什么要单独一个文件：KernelSU 会先把整个 ZIP 解到模块目录，**校验只能发生在解压之后**——
# 包坏了也已经写进去了。我们改成 SKIPUNZIP=1、自己解压（见 customize.sh），
# 于是可以在落盘之前先把整包核对一遍：读的是 ZIP 里的字节，而不是已经解出来的文件。
#
# 这里只用 sha256sum、cut、basename 这些基础工具，保持可在安装环境里跑。

# compare_sha256 <文件或 - > <期望摘要>：只打印 ok / FAILED，便于逐行核对。
compare_sha256() {
    _actual=$(sha256sum "$1" 2>/dev/null | cut -d' ' -f1)
    if [ -n "$_actual" ] && [ "$_actual" = "$2" ]; then
        echo ok
    else
        echo FAILED
    fi
}

# verify_zip <zip> <校验清单>：在解压之前核对整包。
# 清单每行是 "<sha256>  <包内路径>"，由 build.mjs 在打包时按字母序生成。
verify_zip() {
    _zip=$1
    _list=$2
    [ -f "$_zip" ] || { echo "找不到安装包：$_zip"; return 1; }
    [ -f "$_list" ] || { echo "找不到校验清单"; return 1; }
    _count=0
    while IFS= read -r _line; do
        case "$_line" in
            '#'*|'') continue ;;
        esac
        _want=$(echo "$_line" | cut -d' ' -f1)
        _name=$(echo "$_line" | cut -d' ' -f3-)
        [ -n "$_want" ] || continue
        [ -n "$_name" ] || continue
        _result=$(unzip -p "$_zip" "$_name" 2>/dev/null | compare_sha256 - "$_want")
        if [ "$_result" != ok ]; then
            echo "校验失败：$_name"
            return 1
        fi
        _count=$((_count + 1))
    done < "$_list"
    if [ "$_count" -eq 0 ]; then
        echo "校验清单是空的"
        return 1
    fi
    echo "已核对 $_count 个文件"
    return 0
}
