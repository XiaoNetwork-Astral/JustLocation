#!/system/bin/sh

# Compare a file or standard input with its expected SHA-256 digest.
compare_sha256() {
    _actual=$(sha256sum "$1" 2>/dev/null | cut -d' ' -f1)
    if [ -n "$_actual" ] && [ "$_actual" = "$2" ]; then
        echo ok
    else
        echo FAILED
    fi
}

# Verify payload bytes directly from the ZIP against the generated manifest.
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
