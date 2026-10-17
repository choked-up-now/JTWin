@echo off
chcp 65001 >nul
echo =========================================
echo 开始构建 汲湍 (JiTuan) v0.2.0 安装包...
echo =========================================

echo [1/3] 正在编译 Release 版本...
cargo build --release --features full
if %errorlevel% neq 0 (
    echo 编译失败！
    pause
    exit /b
)

echo [2/3] 正在准备打包文件...
if not exist "installer\JiTuan_Release" mkdir "installer\JiTuan_Release"

copy /Y "D:\Rust\target\release\jituan-gui.exe" "installer\JiTuan_Release\" >nul
copy /Y "D:\Rust\target\release\jituan-cli.exe" "installer\JiTuan_Release\" >nul
if exist "LICENSE" (copy /Y "LICENSE" "installer\JiTuan_Release\" >nul) else (echo 缺少 LICENSE 文件！)
if exist "NOTICE" (copy /Y "NOTICE" "installer\JiTuan_Release\" >nul) else (echo 缺少 NOTICE 文件！)

echo [3/3] 正在调用 Inno Setup 生成安装包...
"D:\Inno Setup 6\ISCC.exe" "installer\JiTuan.iss"

if %errorlevel% equ 0 (
    echo =========================================
    echo 构建成功！安装包已生成至: installer\Output\
    echo =========================================
) else (
    echo 打包失败，请检查 Inno Setup 脚本。
)
pause