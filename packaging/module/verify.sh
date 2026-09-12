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
    [ -f "$_zip" ] || { echo "Installation package not found: $_zip"; return 1; }
    [ -f "$_list" ] || { echo "Checksum manifest not found"; return 1; }
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
            echo "Verification failed: $_name"
            return 1
        fi
        _count=$((_count + 1))
    done < "$_list"
    if [ "$_count" -eq 0 ]; then
        echo "Checksum manifest is empty"
        return 1
    fi
    echo "Verified $_count files"
    return 0
}
