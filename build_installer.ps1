# 强制设置控制台编码为 UTF-8
[System.Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

try {
    Write-Host "=========================================" -ForegroundColor Cyan
    Write-Host " 汲湍 (JiTuan) 自动构建与发布流水线" -ForegroundColor Cyan
    Write-Host "=========================================" -ForegroundColor Cyan

    # 1. 提取 Cargo.toml 里的版本号
    $cargoToml = Get-Content "Cargo.toml" -Raw
    $versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"(.*?)"')
    if (-not $versionMatch.Success) {
        throw "无法从 Cargo.toml 读取版本号，请检查格式。"
    }
    $version = $versionMatch.Groups[1].Value
    $tag = "v$version"
    Write-Host "[信息] 当前版本: $tag"

    # 2. 同步版本号到 installer/JiTuan.iss
    Write-Host "[信息] 正在同步版本号到安装脚本..."
    $issPath = "installer\JiTuan.iss"
    if (-not (Test-Path $issPath)) {
        throw "找不到 $issPath 文件！"
    }
    $issContent = Get-Content $issPath -Raw
    $issContent = $issContent -replace '#define MyAppVersion ".*"', "#define MyAppVersion `"$version`""
    [System.IO.File]::WriteAllText((Resolve-Path $issPath), $issContent, (New-Object System.Text.UTF8Encoding($true)))

    # 3. 编译 Release 版本
    Write-Host "`n[1/5] 正在编译 Release 版本..." -ForegroundColor Yellow
    cargo build --release --features full
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败！" }

    # 4. 准备打包文件
    Write-Host "`n[2/5] 正在准备打包文件..." -ForegroundColor Yellow
    $releaseDir = "installer\JiTuan_Release"
    if (-not (Test-Path $releaseDir)) { New-Item -ItemType Directory -Path $releaseDir | Out-Null }

    Copy-Item "D:\Rust\target\release\jituan-gui.exe" -Destination $releaseDir -Force
    Copy-Item "D:\Rust\target\release\jituan-cli.exe" -Destination $releaseDir -Force
    if (Test-Path "LICENSE") { Copy-Item "LICENSE" -Destination $releaseDir -Force }
    if (Test-Path "NOTICE") { Copy-Item "NOTICE" -Destination $releaseDir -Force }

    # 5. 调用 Inno Setup 生成安装包
    Write-Host "`n[3/5] 正在调用 Inno Setup 生成安装包..." -ForegroundColor Yellow
    $isccPath = "D:\Inno Setup 6\ISCC.exe"
    if (-not (Test-Path $isccPath)) { throw "找不到 Inno Setup，请检查路径：$isccPath" }
    & $isccPath "installer\JiTuan.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup 编译失败，请检查 .iss 脚本。" }

    $exePath = "installer\Output\JiTuan_${tag}_Setup.exe"
    if (-not (Test-Path $exePath)) { throw "找不到生成的安装包：$exePath" }

    # 6. Git 提交与推送
    Write-Host "`n[4/5] 正在推送到 AtomGit..." -ForegroundColor Yellow
    git add .
    git diff-index --quiet HEAD
    if ($LASTEXITCODE -ne 0) {
        git commit -m "chore: release $tag"
    } else {
        Write-Host "[Git] 无文件变更，跳过提交。"
    }
    git tag -f $tag
    git push origin main
    git push origin -f $tag

    # 7. 创建 Release 并上传附件
    Write-Host "`n[5/5] 正在创建 AtomGit Release 并上传附件..." -ForegroundColor Yellow

    $secretPath = "..\secret.txt"
    if (-not (Test-Path $secretPath)) { throw "未找到 $secretPath，请把 Token 放在上一级目录。" }

    $token = (Get-Content $secretPath -Raw).Trim()
    if ([string]::IsNullOrEmpty($token)) { throw "读取 Token 失败，secret.txt 内容为空。" }

    Write-Host "[信息] 成功读取 Token，前 5 位: $($token.Substring(0, 5))***"

    $headers = @{
        "PRIVATE-TOKEN" = $token
        "Content-Type"  = "application/json"
    }
    $apiBase = "https://api.atomgit.com/api/v5"
    $repoOwner = "choked-up-now"
    $repoName = "JiTuan"
    $releaseApiUrl = "$apiBase/repos/$repoOwner/$repoName/releases"

    Write-Host "[信息] 正在请求创建 Release API..."
    $body = @{
        tag_name = $tag
        name     = "JiTuan $tag"
        body     = "Auto build by pipeline."
    } | ConvertTo-Json

    $releaseId = $null
    try {
        $response = Invoke-RestMethod -Uri $releaseApiUrl -Method Post -Headers $headers -Body $body -ErrorAction Stop
        $releaseId = $response.id
        Write-Host "[成功] Release 创建成功！ID: $releaseId" -ForegroundColor Green
    } catch {
        $errResponse = $_.Exception.Response
        if ($errResponse.StatusCode -eq [System.Net.HttpStatusCode]::Conflict) {
            Write-Host "[警告] Release 已存在，尝试获取已有 ID..." -ForegroundColor Yellow
            # 【核心修改】获取列表，而不是获取单个 tags
            $listApiUrl = "$apiBase/repos/$repoOwner/$repoName/releases?per_page=100"
            try {
                $releases = Invoke-RestMethod -Uri $listApiUrl -Method Get -Headers $headers -ErrorAction Stop
                # 筛选出 tag 匹配的第一个
                $targetRelease = $releases | Where-Object { $_.tag_name -eq $tag } | Select-Object -First 1
                
                if ($targetRelease) {
                    $releaseId = $targetRelease.id
                    Write-Host "[成功] 获取已有 Release 成功！ID: $releaseId" -ForegroundColor Green
                } else {
                    throw "在 Releases 列表中未找到 $tag"
                }
            } catch {
                throw "无法获取已有 Release: $($_.Exception.Message)"
            }
        } else {
            throw "创建 Release 失败: $($_.Exception.Message)"
        }
    }

    if (-not $releaseId) { throw "无法获取 Release ID" }

    # 8. 上传附件
    Write-Host "[信息] 正在上传安装包 $exePath ..."
    $uploadApiUrl = "$apiBase/repos/$repoOwner/$repoName/releases/$releaseId/attach_files"

    try {
        Add-Type -AssemblyName System.Net.Http
        $client = New-Object System.Net.Http.HttpClient
        $client.DefaultRequestHeaders.Add("PRIVATE-TOKEN", $token)
        
        $multipartContent = New-Object System.Net.Http.MultipartFormDataContent
        $fileStream = [System.IO.File]::OpenRead((Resolve-Path $exePath))
        $fileContent = New-Object System.Net.Http.StreamContent($fileStream)
        $multipartContent.Add($fileContent, "file", (Split-Path $exePath -Leaf))
        
        $result = $client.PostAsync($uploadApiUrl, $multipartContent).Result
        $resultJson = $result.Content.ReadAsStringAsync().Result
        
        if ($result.IsSuccessStatusCode) {
            Write-Host "[成功] 附件上传成功！" -ForegroundColor Green
            Write-Host "=========================================" -ForegroundColor Cyan
            Write-Host "🎉 发版流程圆满结束！" -ForegroundColor Cyan
            Write-Host "=========================================" -ForegroundColor Cyan
        } else {
            throw "附件上传失败，服务器返回: $resultJson"
        }
    } catch {
        Write-Host "[错误] 上传发生异常: $($_.Exception.Message)" -ForegroundColor Red
    } finally {
        if ($fileStream) { $fileStream.Dispose() }
        if ($client) { $client.Dispose() }
    }

} catch {
    Write-Host "`n❌ [致命错误] $($_.Exception.Message)" -ForegroundColor Red
}

Read-Host "`n按回车键退出..."