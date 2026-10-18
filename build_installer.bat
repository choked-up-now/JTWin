@echo off
chcp 65001 >nul
echo =========================================
echo 汲湍 (JiTuan) 自动构建与发布流水线 (调试版)
echo =========================================

:: 1. 提取 Cargo.toml 里的版本号
for /f "tokens=2 delims== " %%i in ('findstr "^version" Cargo.toml') do set VERSION=%%i
set VERSION=%VERSION:"=%
set TAG=v%VERSION%
echo [信息] 当前版本: %TAG%

echo [信息] 正在同步版本号到安装脚本...
powershell -NoProfile -Command "(Get-Content 'installer\JiTuan.iss') -replace '#define MyAppVersion \".*\"', '#define MyAppVersion \"%VERSION%\"' | Set-Content 'installer\JiTuan.iss'"
if %errorlevel% neq 0 (
    echo ❌ 同步版本号失败！
    pause
    exit /b
)

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
if not exist "%EXE_PATH%" (
    echo ❌ 找不到生成的安装包：%EXE_PATH%
    pause
    exit /b
)

:: 5. 执行 Git 提交并推送
echo.
echo [4/5] 正在推送到 AtomGit...
git add .
git diff-index --quiet HEAD
if %errorlevel% neq 0 (
    echo [Git] 发现文件变更，正在提交...
    git commit -m "chore: release %TAG%"
) else (
    echo [Git] 没有文件变更，跳过提交。
)

echo [Git] 强制打标签并推送...
git tag -f %TAG%
git push origin main
git push origin -f %TAG%

:: 6. 全自动发布到 AtomGit Release
echo.
echo [5/5] 正在创建 AtomGit Release 并上传附件...

if not exist "..\secret.txt" (
    echo ⚠️ 未找到 ..\secret.txt (在上一级目录)，跳过自动发布。
    pause
    exit /b
)

echo [信息] 正在读取 Token...
for /f "usebackq delims=" %%i in (`powershell -NoProfile -Command "$t = (Get-Content '..\secret.txt' -Raw).Trim(); if ($t -match '^\xEF\xBB\xBF') { $t = $t.Substring(1) }; $t"`) do set ATOMGIT_TOKEN=%%i

if "%ATOMGIT_TOKEN%"=="" (
    echo [错误] 读取 Token 失败，secret.txt 内容为空或格式不对。
    pause
    exit /b
)

set TOKEN_PREVIEW=%ATOMGIT_TOKEN:~0,5%
echo [信息] 成功读取 Token，前 5 位: %TOKEN_PREVIEW%***

set REPO_OWNER=choked-up-now
set REPO_NAME=JiTuan
set API_URL=https://atomgit.com/api/v5/repos/%REPO_OWNER%/%REPO_NAME%/releases

echo [信息] 正在请求创建 Release API...
echo {"tag_name":"%TAG%","name":"JiTuan %TAG%","body":"Auto build by pipeline."} > temp_release.json

curl -s -X POST "%API_URL%" -H "Content-Type: application/json" -H "Authorization: token %ATOMGIT_TOKEN%" -d @temp_release.json -o temp_resp.json
del temp_release.json

:: 解析 JSON 获取 ID
for /f "usebackq delims=" %%i in (`powershell -NoProfile -Command "$json = Get-Content 'temp_resp.json' -Raw | ConvertFrom-Json; if ($json.id) { $json.id }"`) do set RELEASE_ID=%%i

if "%RELEASE_ID%"=="" (
    echo.
    echo [错误] 创建 Release 失败，以下是服务器返回信息:
    type temp_resp.json
    echo.
    del temp_resp.json
    pause
    exit /b
)
del temp_resp.json

echo [成功] Release 创建成功 (ID: %RELEASE_ID%)，正在上传安装包...

set UPLOAD_URL=https://atomgit.com/api/v5/repos/%REPO_OWNER%/%REPO_NAME%/releases/%RELEASE_ID%/attach_files
curl -s -X POST "%UPLOAD_URL%" -H "Authorization: token %ATOMGIT_TOKEN%" -F "file=@%EXE_PATH%" -o temp_upload.json

findstr "browser_download_url" temp_upload.json >nul
if %errorlevel% equ 0 (
    echo [成功] 附件上传成功！
    echo =========================================
    echo 🎉 发版流程圆满结束！
    echo =========================================
) else (
    echo [错误] 附件上传失败，返回信息:
    type temp_upload.json
)
del temp_upload.json

pause