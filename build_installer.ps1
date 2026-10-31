# 强制设置控制台编码为 UTF-8
[System.Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

try {
    Write-Host "=========================================" -ForegroundColor Cyan
    Write-Host " 汲湍 (JiTuan) 自动构建与发布流水线" -ForegroundColor Cyan
    Write-Host "=========================================" -ForegroundColor Cyan

    # 1. 提取版本号
    $cargoToml = Get-Content "Cargo.toml" -Raw
    $versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"(.*?)"')
    if (-not $versionMatch.Success) { throw "无法从 Cargo.toml 读取版本号" }
    $version = $versionMatch.Groups[1].Value
    $tag = "v${version}"
    Write-Host "[信息] 当前版本: $tag"

    # 2. 同步版本号到 .iss
    Write-Host "[信息] 正在同步版本号到安装脚本..."
    $issPath = "installer\JiTuan.iss"
    if (-not (Test-Path $issPath)) { throw "找不到 $issPath 文件！" }
    
    $issContent = [System.IO.File]::ReadAllText((Resolve-Path $issPath), [System.Text.Encoding]::UTF8)
    $issContent = $issContent -replace '#define MyAppVersion ".*"', "#define MyAppVersion `"$version`""
    [System.IO.File]::WriteAllText((Resolve-Path $issPath), $issContent, (New-Object System.Text.UTF8Encoding($true)))

    $releaseDir = "installer\JiTuan_Release"
    if (-not (Test-Path $releaseDir)) { New-Item -ItemType Directory -Path $releaseDir | Out-Null }

    # 3. 编译 Release 版本（编译两次：完整版 和 精简版）
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
    if (-not (Test-Path $isccPath)) { throw "找不到 Inno Setup，请检查路径：$isccPath" }
    & $isccPath "installer\JiTuan.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup 编译失败。" }

    $exePath = "installer\Output\JiTuan_${tag}_Setup.exe"
    if (-not (Test-Path $exePath)) { throw "找不到生成的安装包：$exePath" }

    # 6. Git 提交与推送
    Write-Host "`n[4/5] 正在推送到 AtomGit..." -ForegroundColor Yellow
    git add .
    git diff-index --quiet HEAD
    if ($LASTEXITCODE -ne 0) { git commit -m "chore: release $tag" } else { Write-Host "[Git] 无文件变更，跳过提交。" }
    git tag -f $tag
    
    $secretPath = "..\secret.txt"
    if (-not (Test-Path $secretPath)) { throw "未找到 $secretPath，请把 Token 放在上一级目录。" }
    $token = (Get-Content $secretPath -Raw).Trim()
    if ([string]::IsNullOrEmpty($token)) { throw "读取 Token 失败，secret.txt 内容为空。" }

    $pushUrl = "https://oauth2:${token}@atomgit.com/choked-up-now/JiTuan-win.git"
    git push $pushUrl main -f
    git push $pushUrl $tag -f
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[警告] Git 推送失败，可能是因为没有文件变更或权限问题。" -ForegroundColor Yellow
    } else {
        Write-Host "[成功] 代码和 Tag 已成功推送到 AtomGit。" -ForegroundColor Green
    }

    # 7. 创建 Release 并上传附件
    Write-Host "`n[5/5] 正在创建 AtomGit Release 并上传附件..." -ForegroundColor Yellow

    $headers = @{ "PRIVATE-TOKEN" = $token }
    $apiBase = "https://api.atomgit.com/api/v5"
    $repoOwner = "choked-up-now"
    $repoName = "JiTuan-win"
    $releaseApiUrl = "${apiBase}/repos/${repoOwner}/${repoName}/releases"

    Write-Host "[信息] 正在创建 Release..."
    
    $notesPath = "release_notes.md"
    $releaseBody = "Auto build by pipeline."
    if (Test-Path $notesPath) {
        $releaseBody = [System.IO.File]::ReadAllText((Resolve-Path $notesPath), [System.Text.Encoding]::UTF8).Trim()
    }

    $dateStr = Get-Date -Format "yyyyMMdd"
    $releaseTitle = "汲湍 $tag $dateStr"
    $latestCommit = (git rev-parse HEAD).Trim()

    $isPrerelease = $version -match "-"

    $bodyObject = @{
        tag_name         = $tag
        name             = $releaseTitle
        body             = $releaseBody
        target_commitish = $latestCommit
    }

    if ($isPrerelease) {
        Write-Host "[信息] 检测到预发布版本标签 ($version)，将标记为 Pre-release。" -ForegroundColor Yellow
        $bodyObject["prerelease"] = $true
        $bodyObject["release_status"] = "pre"
    } else {
        Write-Host "[信息] 正式版本 ($version)，将标记为 Latest。" -ForegroundColor Green
        $bodyObject["prerelease"] = $false
        $bodyObject["release_status"] = "latest"
    }

    $jsonBody = $bodyObject | ConvertTo-Json -Depth 10
    $utf8Bytes = [System.Text.Encoding]::UTF8.GetBytes($jsonBody)

    try {
        $response = Invoke-RestMethod -Uri $releaseApiUrl -Method Post -Headers $headers -ContentType "application/json; charset=utf-8" -Body $utf8Bytes -ErrorAction Stop
        Write-Host "[成功] Release 创建成功！" -ForegroundColor Green
    } catch {
        $statusCode = $_.Exception.Response.StatusCode.value__
        if ($statusCode -eq 409) {
            Write-Host "[警告] Release 已存在，尝试更新已有 Release 标记..." -ForegroundColor Yellow
            $updateApiUrl = "${apiBase}/repos/${repoOwner}/${repoName}/releases/${tag}"
            try {
                $patchResponse = Invoke-RestMethod -Uri $updateApiUrl -Method Patch -Headers $headers -ContentType "application/json; charset=utf-8" -Body $utf8Bytes -ErrorAction Stop
                Write-Host "[成功] 已有 Release 已更新！" -ForegroundColor Green
            } catch {
                Write-Host "[警告] 无法更新已有 Release 标记，继续执行上传。" -ForegroundColor Yellow
            }
        } else {
            throw "创建 Release 失败: $($_.Exception.Message)"
        }
    }

    # 8. 上传附件
    $fileName = Split-Path $exePath -Leaf
    Write-Host "[信息] 正在获取附件上传地址..."
    $uploadUrlApi = "${apiBase}/repos/${repoOwner}/${repoName}/releases/${tag}/upload_url?file_name=${fileName}"
    
    try {
        $uploadMeta = Invoke-RestMethod -Uri $uploadUrlApi -Method Get -Headers $headers -ErrorAction Stop
        $signedUrl = $uploadMeta.url
        $uploadHeaders = @{}
        $uploadMeta.headers.PSObject.Properties | ForEach-Object { $uploadHeaders[$_.Name] = $_.Value }

        Write-Host "[信息] 正在上传安装包（PUT 请求）..."
        
        Add-Type -AssemblyName System.Net.Http
        $client = New-Object System.Net.Http.HttpClient
        
        foreach ($key in $uploadHeaders.Keys) {
            if ($key -eq "Content-Type") { continue }
            $client.DefaultRequestHeaders.Add($key, $uploadHeaders[$key])
        }

        $fileStream = [System.IO.File]::OpenRead((Resolve-Path $exePath))
        $streamContent = New-Object System.Net.Http.StreamContent($fileStream)
        $contentType = if ($uploadHeaders["Content-Type"]) { $uploadHeaders["Content-Type"] } else { "application/octet-stream" }
        $streamContent.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::Parse($contentType)

        $result = $client.PutAsync($signedUrl, $streamContent).Result
        $resultJson = $result.Content.ReadAsStringAsync().Result

        if ($result.IsSuccessStatusCode) {
            Write-Host "[成功] 附件上传成功！" -ForegroundColor Green
            Write-Host "=========================================" -ForegroundColor Cyan
            Write-Host "🎉 发版流程圆满结束！" -ForegroundColor Cyan
            Write-Host "=========================================" -ForegroundColor Cyan
        } else {
            throw "附件上传失败（状态码: $($result.StatusCode)），服务器返回: $resultJson"
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