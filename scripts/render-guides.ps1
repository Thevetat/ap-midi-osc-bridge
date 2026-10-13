param(
    [Parameter(Mandatory = $true)][string]$Browser
)
$ErrorActionPreference = 'Stop'
$browserPath = (Get-Item -LiteralPath $Browser).FullName
$project = Split-Path $PSScriptRoot -Parent
$work = Join-Path ([System.IO.Path]::GetTempPath()) "ap-midi-osc-guides-$([guid]::NewGuid().ToString('N'))"
$guides = @('CONTROL-GUIDE', 'LAUNCH-CONTROL-3-GUIDE')
New-Item -ItemType Directory -Path $work | Out-Null
try {
    foreach ($guide in $guides) {
        $source = (Get-Item -LiteralPath (Join-Path $project "$guide.html")).FullName
        $url = ([System.Uri]::new($source)).AbsoluteUri
        $pdf = Join-Path $work "$guide.pdf"
        $profile = Join-Path $work "$guide-browser"
        $errors = Join-Path $work "$guide.stderr"
        $arguments = @(
            '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
            '--disable-background-networking', '--no-pdf-header-footer',
            "--user-data-dir=`"$profile`"", "--print-to-pdf=`"$pdf`"", "`"$url`""
        )
        $process = Start-Process -FilePath $browserPath -ArgumentList $arguments -Wait -PassThru `
            -RedirectStandardOutput (Join-Path $work "$guide.stdout") -RedirectStandardError $errors
        if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $pdf) -or (Get-Item -LiteralPath $pdf).Length -eq 0) {
            throw "Rendering $guide failed (exit $($process.ExitCode)): $([System.IO.File]::ReadAllText($errors))"
        }
    }
    # Do not replace either tracked PDF unless both renders succeeded.
    foreach ($guide in $guides) {
        Copy-Item -LiteralPath (Join-Path $work "$guide.pdf") -Destination (Join-Path $project "$guide.pdf") -Force
        Write-Host "Rendered $guide.pdf"
    }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force
}
