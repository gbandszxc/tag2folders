#Requires -Version 5.1
# tag2folders Windows 打包脚本: release 二进制 → MSI(WiX v5)
#
# 用法:
#   powershell -File scripts\build-msi.ps1               # cargo build --release 后打包
#   $env:T2F_SKIP_BUILD=1; powershell -File scripts\build-msi.ps1   # 复用 target/release 已有二进制
#   $env:T2F_BIN='D:\path\tag2folders.exe'; powershell ...           # 用指定二进制打包(如 CI 产物)
#
# 产物: target\msi\tag2folders_<version>_<arch>.msi(版本取自 Cargo.toml,架构取自 rustc host)
# 详见 docs/PACKAGING.md。
#
# 依赖:cargo + WiX v5。wix 不在 PATH 时自动下载官方 wix-cli MSI 到
# %LOCALAPPDATA%\tag2folders\wix-cli\5.0.2 并以 msiexec /a 免管理员解包使用
# (仅需 .NET 运行时,无需 .NET SDK)。
$ErrorActionPreference = 'Stop'

Set-Location (Join-Path $PSScriptRoot '..')
$repoRoot = (Get-Location).Path

$AppName    = 'tag2folders'
$WixVersion = '5.0.2'
$WixUrl     = "https://github.com/wixtoolset/wix/releases/download/v$WixVersion/wix-cli-x64.msi"
$WixCache   = Join-Path $env:LOCALAPPDATA "tag2folders\wix-cli\$WixVersion"
$MsiDir     = Join-Path $repoRoot 'target\msi'

# 1) 版本(取 Cargo.toml 的 [package] version;MSI 仅支持 x.y.z 三段,多余段截断)
$Version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $Version) { throw '无法从 Cargo.toml 解析版本' }
$MsiVersion = ($Version -split '\.' | Select-Object -First 3) -join '.'

# 2) 架构(rustc host triple → wix -arch)
$RustHost = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
switch -Regex ($RustHost) {
    '^x86_64'   { $Arch = 'x64';   break }
    '^aarch64'  { $Arch = 'arm64'; break }
    default { throw "不支持的 host: $RustHost" }
}

# 3) 二进制
$Bin = if ($env:T2F_BIN) { $env:T2F_BIN } else { Join-Path $repoRoot "target\release\$AppName.exe" }
if (-not $env:T2F_BIN -and -not $env:T2F_SKIP_BUILD) { cargo build --release }
if (-not (Test-Path $Bin)) { throw "二进制不存在: $Bin" }
Write-Host "==> 二进制: $Bin ($([math]::Round((Get-Item $Bin).Length / 1MB, 1)) MB, $RustHost)"

# 4) 图标(缓存 assets/app.ico;缺失时从 assets/app-icon.png 用 System.Drawing 重建)
$IconPng = Join-Path $repoRoot 'assets\app-icon.png'
$Icon    = Join-Path $repoRoot 'assets\app.ico'
if (-not (Test-Path $Icon)) {
    Write-Host '==> 生成 assets/app.ico(建议提交缓存;有 python+PIL 时可手工高质量重建)'
    Add-Type -AssemblyName System.Drawing
    $src = [System.Drawing.Image]::FromFile($IconPng)
    $payloads = @()   # @{ Size; Bytes }
    foreach ($s in 16, 24, 32, 48, 64, 128, 256) {
        $bmp = New-Object System.Drawing.Bitmap $s, $s
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($src, 0, 0, $s, $s); $g.Dispose()
        if ($s -eq 256) {   # 256 用 PNG 压缩条目(Vista+)
            $ms = New-Object System.IO.MemoryStream
            $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
            $payloads += @{ Size = $s; Bytes = $ms.ToArray() }
        } else {            # 小尺寸用 32bpp BMP 条目(BITMAPINFOHEADER + 自下而上 BGRA + 全零 AND 掩码)
            $xor = New-Object System.IO.MemoryStream
            $hdr = [BitConverter]::GetBytes([uint32]40) + [BitConverter]::GetBytes([int32]$s) +
                   [BitConverter]::GetBytes([int32]($s * 2)) + (New-Object byte[] 2) +
                   [BitConverter]::GetBytes([uint16]1) + [BitConverter]::GetBytes([uint16]32) +
                   (New-Object byte[] 16)
            $xor.Write($hdr, 0, $hdr.Length)
            for ($y = $s - 1; $y -ge 0; $y--) {
                for ($x = 0; $x -lt $s; $x++) {
                    $c = $bmp.GetPixel($x, $y)
                    $xor.Write(@([byte]$c.B, [byte]$c.G, [byte]$c.R, [byte]$c.A), 0, 4)
                }
            }
            $andRow = [math]::Ceiling($s / 8 / 4) * 4
            $and = New-Object byte[] ($andRow * $s)
            $xor.Write($and, 0, $and.Length)
            $payloads += @{ Size = $s; Bytes = $xor.ToArray() }
        }
        $bmp.Dispose()
    }
    $src.Dispose()
    $out = New-Object System.IO.MemoryStream
    $out.Write([BitConverter]::GetBytes([uint16]0) + [BitConverter]::GetBytes([uint16]1) +
               [BitConverter]::GetBytes([uint16]$payloads.Count), 0, 6)
    $offset = 6 + 16 * $payloads.Count
    foreach ($p in $payloads) {
        $b = if ($p.Size -eq 256) { 0 } else { $p.Size }
        $entry = @([byte]$b, [byte]$b, 0, 0) + [BitConverter]::GetBytes([uint16]1) +
                 [BitConverter]::GetBytes([uint16]32) + [BitConverter]::GetBytes([uint32]$p.Bytes.Length) +
                 [BitConverter]::GetBytes([uint32]$offset)
        $out.Write($entry, 0, 16); $offset += $p.Bytes.Length
    }
    foreach ($p in $payloads) { $out.Write($p.Bytes, 0, $p.Bytes.Length) }
    [System.IO.File]::WriteAllBytes($Icon, $out.ToArray())
}
if (-not (Test-Path $Icon)) { throw "图标不存在: $Icon" }

# 5) wix(优先 PATH / T2F_WIX,否则用本地缓存的解包版,再不行下载 wix-cli MSI 免管理员解包)
function Get-Wix {
    if ($env:T2F_WIX) { return $env:T2F_WIX }
    $onPath = Get-Command wix -ErrorAction SilentlyContinue
    $exe = Join-Path $WixCache "unpack\PFiles64\WiX Toolset v$($WixVersion[0]).0\bin\wix.exe"
    if (Test-Path $exe) { return $exe }
    Write-Host "==> 下载 WiX CLI v$WixVersion(仅首次,~12MB)"
    New-Item -ItemType Directory -Force -Path $WixCache | Out-Null
    $msi = Join-Path $WixCache 'wix-cli-x64.msi'
    Invoke-WebRequest -Uri $WixUrl -OutFile $msi
    # /a 管理员映像解包到独立子目录(解包会把 MSI 副本写入目标目录,不能与源同目录):
    # 只取文件,不安装、不需要管理员权限
    $p = Start-Process msiexec.exe -ArgumentList "/a `"$msi`" /qn TARGETDIR=`"$(Join-Path $WixCache 'unpack')`"" -Wait -PassThru
    if ($p.ExitCode -ne 0 -or -not (Test-Path $exe)) { throw "wix-cli MSI 解包失败(0x$('{0:X}' -f $p.ExitCode))" }
    return $exe
}
$wix = Get-Wix
Write-Host "==> wix: $wix"

# 6) 编译 MSI(变量经 -d 注入,见 scripts/tag2folders.wxs)
New-Item -ItemType Directory -Force -Path $MsiDir | Out-Null
$Msi = Join-Path $MsiDir "${AppName}_${MsiVersion}_${Arch}.msi"
if (Test-Path $Msi) { Remove-Item $Msi }
& $wix build -arch $Arch -d "Version=$MsiVersion" -d "BinFile=$Bin" -d "IconFile=$Icon" `
    (Join-Path $repoRoot 'scripts\tag2folders.wxs') -o $Msi
if ($LASTEXITCODE -ne 0) { throw "wix build 失败($LASTEXITCODE)" }

# 7) 校验(msiexec /a 管理员映像到临时目录:验证文件表/cab 完整性,不触碰系统)
$adminImage = Join-Path ([System.IO.Path]::GetTempPath()) "t2f-msi-check"
if (Test-Path $adminImage) { Remove-Item $adminImage -Recurse -Force }
$p = Start-Process msiexec.exe -ArgumentList "/a `"$Msi`" /qn TARGETDIR=`"$adminImage`"" -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "MSI 校验失败(0x$('{0:X}' -f $p.ExitCode))" }
$installed = Get-ChildItem $adminImage -Recurse -Filter *.exe | Where-Object Name -eq 'tag2folders.exe'
if (-not $installed) { throw 'MSI 校验失败:映像中未找到 tag2folders.exe' }
Remove-Item $adminImage -Recurse -Force
Write-Host '==> MSI 校验通过'

Write-Host "==> 完成: $Msi ($([math]::Round((Get-Item $Msi).Length / 1MB, 1)) MB)"
