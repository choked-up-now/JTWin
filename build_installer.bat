@echo off
chcp 65001 >nul
echo =========================================
echo 汲湍 (JiTuan) 自动构建与发布流水线
echo =========================================

:: 1. 自动提取 Cargo.toml 里的版本号
for /f "tokens=2 delims== " %%i in ('findstr "^version" Cargo.toml') do set VERSION=%%i
set VERSION=%VERSION:"=%
set TAG=v%VERSION%
echo [信息] 当前版本: %TAG%

:: 2. 编译 Release 版本
echo.
echo [1/5] 正在编译 Release 版本...
cargo build --release --features full
if %errorlevel% neq 0 (
    echo ❌ 编译失败！
    pause
    exit /b
)

:: 3. 准备打包文件
echo.
echo [2/5] 正在准备打包文件...
if not exist "installer\JiTuan_Release" mkdir "installer\JiTuan_Release"
copy /Y "D:\Rust\target\release\jituan-gui.exe" "installer\JiTuan_Release\" >nul
copy /Y "D:\Rust\target\release\jituan-cli.exe" "installer\JiTuan_Release\" >nul
if exist "LICENSE" (copy /Y "LICENSE" "installer\JiTuan_Release\" >nul) else (echo 缺少 LICENSE 文件！)
if exist "NOTICE" (copy /Y "NOTICE" "installer\JiTuan_Release\" >nul) else (echo 缺少 NOTICE 文件！)

:: 4. 调用 Inno Setup 生成安装包
echo.
echo [3/5] 正在调用 Inno Setup 生成安装包...
"D:\Inno Setup 6\ISCC.exe" "installer\JiTuan.iss"
if %errorlevel% neq 0 (
    echo ❌ 打包失败，请检查 Inno Setup 脚本。
    pause
    exit /b
)

set EXE_PATH=installer\Output\JiTuan_%TAG%_Setup.exe

:: 5. 执行 Git 提交并推送
echo.
echo [4/5] 正在推送到 AtomGit...
git add .
git commit -m "chore: release %TAG%"
git tag %TAG%
git push origin main
git push origin %TAG%

:: 6. 全自动发布到 AtomGit Release
echo.
echo [5/5] 正在创建 AtomGit Release 并上传附件...

:: 读取上一级目录的 secret.txt
if not exist "..\secret.txt" (
    echo ⚠️ 未找到 ..\secret.txt，跳过自动发布。
    pause
    exit /b
)

:: 使用 PowerShell 提取 Token 并去除空白字符
for /f "usebackq delims=" %%i in (`powershell -NoProfile -Command "(Get-Content '..\secret.txt' -Raw).Trim()"`) do set ATOMGIT_TOKEN=%%i

set REPO_OWNER=choked-up-now
set REPO_NAME=JiTuan
set API_URL=https://atomgit.com/api/v5/repos/%REPO_OWNER%/%REPO_NAME%/releases

:: 使用 curl 发送 POST 请求创建 Release (生成临时 json 文件避免转义问题)
echo {"tag_name":"%TAG%","name":"汲湍 %TAG% 发布","body":"本次更新由自动化流水线自动构建。"} > temp_release.json
curl -s -X POST "%API_URL%" -H "Content-Type: application/json" -H "Authorization: token %ATOMGIT_TOKEN%" -d @temp_release.json -o temp_resp.json
del temp_release.json

:: 使用 PowerShell 从 JSON 响应中提取 Release ID
for /f "usebackq delims=" %%i in (`powershell -NoProfile -Command "(Get-Content temp_resp.json -Raw | ConvertFrom-Json).id"`) do set RELEASE_ID=%%i
del temp_resp.json

if "%RELEASE_ID%"=="" (
    echo ❌ 创建 Release 失败，请检查 Token 权限或 API 是否更改。
    echo 可能是该版本已存在，请去网页端检查。
    pause
    exit /b
)

echo ✅ Release 创建成功 (ID: %RELEASE_ID%)，正在上传安装包...

:: 使用 curl 上传附件
set UPLOAD_URL=https://atomgit.com/api/v5/repos/%REPO_OWNER%/%REPO_NAME%/releases/%RELEASE_ID%/attach_files
curl -s -X POST "%UPLOAD_URL%" -H "Authorization: token %ATOMGIT_TOKEN%" -F "file=@%EXE_PATH%" -o temp_upload.json

findstr "browser_download_url" temp_upload.json >nul
if %errorlevel% equ 0 (
    echo ✅ 附件上传成功！
    echo 🎉 发版流程圆满结束！
) else (
    echo ❌ 附件上传失败，返回信息:
    type temp_upload.json
)
del temp_upload.json

echo =========================================
pause