# 安装脚本。KernelSU 在运行它之前已经完成解压、属主与 SELinux 标签设置。
# 模块布局沿用 5ec1cff/zygisk-module-template；文案与完整性校验集中在 util_functions.sh。
. "$MODPATH/util_functions.sh"

require_kernelsu
require_arm64
require_api

msg "$MSG_INSTALLING"
msg "$MSG_NEED_ZYGISK"

# 带 .sha256 清单的正式包会在这里真正逐个校验；直接从源码目录打包时清单不存在，
# verify_file 对缺失清单按通过处理，不会把开发流程卡住。
if verify_tree "$MODPATH"; then
    msg "$MSG_VERIFY_OK"
else
    fail "$MSG_VERIFY_FAIL"
fi

set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/bin/justlocationd" 0 0 0755
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/uninstall.sh" 0 0 0755
set_perm "$MODPATH/util_functions.sh" 0 0 0644

msg "$MSG_DONE"
