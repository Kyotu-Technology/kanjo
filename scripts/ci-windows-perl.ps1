# The vendored OpenSSL is configured by a Perl script that needs a native
# Windows Perl; the MSYS perl that ships with Git breaks it. Prefer the
# runner's Strawberry Perl, install it if absent, and point openssl-src at it.
$ErrorActionPreference = 'Stop'
$perl = 'C:\Strawberry\perl\bin\perl.exe'
if (-not (Test-Path -LiteralPath $perl)) {
    choco install strawberryperl --no-progress -y
}
if (-not (Test-Path -LiteralPath $perl)) {
    throw "Strawberry Perl not found at $perl"
}
& $perl -v | Select-Object -First 2
Add-Content -LiteralPath $env:GITHUB_ENV -Value "OPENSSL_SRC_PERL=$perl" -Encoding utf8
