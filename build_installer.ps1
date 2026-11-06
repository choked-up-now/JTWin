# ==============================================================================
# 汲湍 (JiTuan) 自动构建与发布流水线 (PowerShell 7 极简版)
# ==============================================================================

$OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " 汲湍 (JiTuan) 自动构建与发布流水线" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

try {
    # 1. 提取版本号
    $cargoToml = Get-Content "Cargo.toml" -Raw
    if ($cargoToml -match '(?m)^version\s*=\s*"(.*?)"') {
        $version = $Matches[1]
    } else {
        throw "无法从 Cargo.toml 读取版本号"
    }
    $tag = "v${version}"
    Write-Host "[信息] 当前版本: $tag"

    # 2. 同步版本号到 .iss (Inno Setup 必须带 BOM，所以这里用 .NET 强制写入)
    Write-Host "[信息] 正在同步版本号到安装脚本..."
    $issPath = "installer\JiTuan.iss"
    if (-not (Test-Path $issPath)) { throw "找不到 $issPath 文件！" }
    
    $issContent = Get-Content $issPath -Raw
    $issContent = $issContent -replace '#define MyAppVersion ".*"', "#define MyAppVersion `"$version`""
    [System.IO.File]::WriteAllText((Resolve-Path $issPath), $issContent, (New-Object System.Text.UTF8Encoding($true)))

    $releaseDir = "installer\JiTuan_Release"
    if (-not (Test-Path $releaseDir)) { New-Item -ItemType Directory -Path $releaseDir | Out-Null }

    # 3. 编译 Release 版本
    Write-Host "`n[1/5] 正在编译 Release 版本..." -ForegroundColor Yellow
    
    Write-Host "-> 编译完整版 (Full, 包含 P2P)..." -ForegroundColor Cyan
    cargo build --release --features full
    if ($LASTEXITCODE -ne 0) { throw "编译完整版失败！" }
    Copy-Item "D:\Rust\target\release\jituan-gui.exe" -Destination "$releaseDir\jituan-gui-full.exe" -Force
    Copy-Item "D:\Rust\target\release\jituan-cli.exe" -Destination "$releaseDir\jituan-cli-full.exe" -Force

    Write-Host "-> 编译精简版 (Lite, 纯 HTTP)..." -ForegroundColor Cyan
    cargo build --release --features cli,gui
    if ($LASTEXITCODE -ne 0) { throw "编译精简版失败！" }
    Copy-Item "D:\Rust\target\release\jituan-gui.exe" -Destination "$releaseDir\jituan-gui-lite.exe" -Force
    Copy-Item "D:\Rust\target\release\jituan-cli.exe" -Destination "$releaseDir\jituan-cli-lite.exe" -Force

    # 4. 准备打包文件
    Write-Host "`n[2/5] 正在准备打包文件..." -ForegroundColor Yellow
    if (Test-Path "LICENSE") { Copy-Item "LICENSE" -Destination $releaseDir -Force }
    if (Test-Path "NOTICE") { Copy-Item "NOTICE" -Destination $releaseDir -Force }

    # 5. 调用 Inno Setup 生成安装包
    Write-Host "`n[3/5] 正在调用 Inno Setup 生成安装包..." -ForegroundColor Yellow
    $isccPath = "D:\Inno Setup 6\ISCC.exe"
    if (-not (Test-Path $isccPath)) { throw "找不到 Inno Setup：$isccPath" }
    & $isccPath "installer\JiTuan.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup 编译失败。" }

    $exePath = "installer\Output\JiTuan_${tag}_Setup.exe"
    if (-not (Test-Path $exePath)) { throw "找不到生成的安装包：$exePath" }

    # 6. Git 提交与推送
    Write-Host "`n[4/5] 正在推送到 AtomGit (JTWin)..." -ForegroundColor Yellow
    git add .
    git diff-index --quiet HEAD
    if ($LASTEXITCODE -ne 0) { git commit -m "chore: release $tag" } else { Write-Host "[Git] 无文件变更，跳过提交。" }
    git tag -f $tag
    
    $secretPath = "..\secret.txt"
    if (-not (Test-Path $secretPath)) { throw "未找到 $secretPath" }
    $token = (Get-Content $secretPath -Raw).Trim()
    if ([string]::IsNullOrEmpty($token)) { throw "读取 Token 失败" }

    $pushUrl = "https://oauth2:${token}@atomgit.com/choked-up-now/JTWin.git"
    git push $pushUrl main -f
    git push $pushUrl $tag -f
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[警告] Git 推送失败。" -ForegroundColor Yellow
    } else {
        Write-Host "[成功] 代码和 Tag 已成功推送到新仓库。" -ForegroundColor Green
    }

    # 7. 创建 Release 并上传附件
    Write-Host "`n[5/5] 正在创建 AtomGit Release 并上传附件..." -ForegroundColor Yellow

    $headers = @{ "PRIVATE-TOKEN" = $token }
    $apiBase = "https://api.atomgit.com/api/v5"
    $repoOwner = "choked-up-now"
    $repoName = "JTWin"
    $releaseApiUrl = "${apiBase}/repos/${repoOwner}/${repoName}/releases"

    $notesPath = "release_notes.md"
    $releaseBody = if (Test-Path $notesPath) { Get-Content $notesPath -Raw } else { "Auto build by pipeline." }

    $dateStr = Get-Date -Format "yyyyMMdd"
    $releaseTitle = "汲湍 $tag $dateStr"
    $latestCommit = (git rev-parse HEAD).Trim()
    $isPrerelease = $version -match "-"

    $bodyObject = @{
        tag_name         = $tag
        name             = $releaseTitle
        body             = $releaseBody
        target_commitish = $latestCommit
        prerelease       = $isPrerelease
    }
    
    # 如果不是预发布，才打上 latest 标签
    if (-not $isPrerelease) {
        $bodyObject["release_status"] = "latest"
        Write-Host "[信息] 正式版本，将标记为 Latest。" -ForegroundColor Green
    } else {
        Write-Host "[信息] 预发布版本，将标记为 Pre-release。" -ForegroundColor Yellow
    }

    # PS7 的 Invoke-RestMethod 直接传 JSON 字符串即可，默认 UTF-8
    $jsonBody = $bodyObject | ConvertTo-Json -Depth 10

    try {
        $null = Invoke-RestMethod -Uri $releaseApiUrl -Method Post -Headers $headers -ContentType "application/json; charset=utf-8" -Body $jsonBody -ErrorAction Stop
        Write-Host "[成功] Release 创建成功！" -ForegroundColor Green
    } catch {
        $statusCode = $_.Exception.Response.StatusCode.value__
        if ($statusCode -eq 409) {
            Write-Host "[警告] Release 已存在，正在尝试更新..." -ForegroundColor Yellow
            $updateApiUrl = "${apiBase}/repos/${repoOwner}/${repoName}/releases/${tag}"
            try {
                $null = Invoke-RestMethod -Uri $updateApiUrl -Method Patch -Headers $headers -ContentType "application/json; charset=utf-8" -Body $jsonBody -ErrorAction Stop
                Write-Host "[成功] 已有 Release 已更新！" -ForegroundColor Green
            } catch {
                Write-Host "[警告] 无法更新已有 Release 标记，继续执行。" -ForegroundColor Yellow
            }
        } else {
            throw "创建 Release 失败: $($_.Exception.Message)"
        }
    }

    # 8. 上传附件
    $fileName = Split-Path $exePath -Leaf
    $uploadUrlApi = "${apiBase}/repos/${repoOwner}/${repoName}/releases/${tag}/upload_url?file_name=${fileName}"
    
    try {
        $uploadMeta = Invoke-RestMethod -Uri $uploadUrlApi -Method Get -Headers $headers -ErrorAction Stop
        Write-Host "[信息] 正在上传安装包（PUT 请求）..." -ForegroundColor Cyan
        
        # PS7 下也可以直接使用 Invoke-RestMethod 传文件流，但为了保留签名头的准确性，继续用 .NET 方法
        Add-Type -AssemblyName System.Net.Http
        $client = New-Object System.Net.Http.HttpClient
        
        foreach ($key in $uploadMeta.headers.PSObject.Properties.Name) {
            if ($key -eq "Content-Type") { continue }
            $client.DefaultRequestHeaders.Add($key, $uploadMeta.headers.$key)
        }

        $fileStream = [System.IO.File]::OpenRead((Resolve-Path $exePath))
        $streamContent = New-Object System.Net.Http.StreamContent($fileStream)
        $streamContent.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::Parse("application/octet-stream")

        $result = $client.PutAsync($uploadMeta.url, $streamContent).Result

        if ($result.IsSuccessStatusCode) {
            Write-Host "[成功] 附件上传成功！" -ForegroundColor Green
            Write-Host "=========================================" -ForegroundColor Cyan
            Write-Host "🎉 发版流程圆满结束！" -ForegroundColor Cyan
            Write-Host "=========================================" -ForegroundColor Cyan
        } else {
            throw "附件上传失败（状态码: $($result.StatusCode)）"
        }
    } catch {
        Write-Host "[错误] 上传过程发生异常: $($_.Exception.Message)" -ForegroundColor Red
    } finally {
        if ($fileStream) { $fileStream.Dispose() }
        if ($client) { $client.Dispose() }
    }

} catch {
    Write-Host "`n❌ [致命错误] $($_.Exception.Message)" -ForegroundColor Red
}

Read-Host "`n按回车键退出..."