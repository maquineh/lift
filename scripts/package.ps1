<#
EN: Build the host CLI and archive its executable, bilingual guides, and examples.
The executable embeds core sources; Rust/Cargo and a host linker are still needed.
PT-BR: Compila a CLI para o host e empacota executável, guias bilíngues e exemplos.
O executável embute o core; Rust/Cargo e um linker do host continuam necessários.
#>
param([string]$OutputDirectory = (Join-Path $PSScriptRoot "../dist"))

$ErrorActionPreference = "Stop"
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$distributionRoot = [System.IO.Path]::GetFullPath($OutputDirectory)
Push-Location $projectRoot
try {
    $hostDetails = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw "rustc -vV failed / falhou" }
    $hostTriple = ($hostDetails | Where-Object { $_ -like "host: *" }) -replace "^host: ", ""
    if (-not $hostTriple) { throw "Rust host was not reported / host Rust não informado" }
    $metadataText = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed / falhou" }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object { $_.name -eq "truxt_cli" }).version
    $targetDirectory = Join-Path $projectRoot "target"
    & cargo build --release --locked -p truxt_cli --target $hostTriple --target-dir $targetDirectory
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed / falhou" }

    $isWindowsHost = [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT
    $executableName = if ($isWindowsHost) { "truxt.exe" } else { "truxt" }
    $executable = Join-Path $targetDirectory "$hostTriple/release/$executableName"
    New-Item -ItemType Directory -Path $distributionRoot -Force | Out-Null
    $stagingDirectory = [System.IO.Path]::GetFullPath((Join-Path $distributionRoot (".truxt-package-" + [guid]::NewGuid().ToString("N"))))
    New-Item -ItemType Directory -Path $stagingDirectory | Out-Null
    try {
        Copy-Item -LiteralPath $executable -Destination $stagingDirectory
        Copy-Item -LiteralPath (Join-Path $projectRoot "README.md"), (Join-Path $projectRoot "README.pt-BR.md") -Destination $stagingDirectory
        Copy-Item -LiteralPath (Join-Path $projectRoot "docs") -Destination $stagingDirectory -Recurse
        $examplesDirectory = Join-Path $stagingDirectory "examples"
        New-Item -ItemType Directory -Path $examplesDirectory | Out-Null
        Get-ChildItem -LiteralPath (Join-Path $projectRoot "examples") -Filter "*.truxt" -File | Copy-Item -Destination $examplesDirectory
        $coreGuideDirectory = Join-Path $stagingDirectory "stdlib/truxt_core"
        New-Item -ItemType Directory -Path $coreGuideDirectory -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $projectRoot "stdlib/truxt_core/README.md"), (Join-Path $projectRoot "stdlib/truxt_core/README.pt-BR.md") -Destination $coreGuideDirectory
        $archive = Join-Path $distributionRoot "truxt-$version-$hostTriple.zip"
        Compress-Archive -Path (Join-Path $stagingDirectory "*") -DestinationPath $archive -Force
        Write-Output $archive
    }
    finally {
        # EN: Verify the absolute cleanup target stays inside this output directory.
        # PT-BR: Verifica que o alvo absoluto de limpeza fica neste diretório de saída.
        $stagingParent = [System.IO.Path]::GetDirectoryName($stagingDirectory)
        if ($stagingParent -eq $distributionRoot -and [System.IO.Path]::GetFileName($stagingDirectory).StartsWith(".truxt-package-")) {
            Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
        }
    }
}
finally { Pop-Location }
