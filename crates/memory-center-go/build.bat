@echo off
REM ============================================================================
REM MemoryCenter Go binding build/test script (Windows mingw-w64 gcc)
REM
REM Background:
REM   - Go cgo on Windows requires mingw-w64 gcc (MSVC cannot compile Go
REM     runtime/cgo GCC-specific syntax like __attribute__, asm).
REM   - mingw ld and gendef have poor support for non-ASCII paths, so we copy
REM     everything to an ASCII-only temp dir and run the full flow there.
REM
REM Prerequisites:
REM   - Rust (cargo build --release -p memory-center-ffi)
REM   - Go 1.21+ (1.26+ recommended)
REM   - mingw-w64 gcc (winget install BrechtSanders.WinLibs.POSIX.UCRT)
REM     - Requires gendef.exe and dlltool.exe (bundled with WinLibs)
REM
REM Flow:
REM   1. cargo build --release -p memory-center-ffi (produces dll)
REM   2. Copy dll + Go sources to %TEMP%\memory-center-go-build (ASCII-only)
REM   3. gendef + dlltool to produce mingw-compatible libmemory_center.dll.a
REM   4. Run go test -v ./... in the temp dir
REM
REM Usage:
REM   cd crates\memory-center-go
REM   build.bat
REM ============================================================================

setlocal
cd /d %~dp0

REM Detect gcc
where gcc >nul 2>&1
if errorlevel 1 (
    echo [ERROR] gcc not found. Install mingw-w64:
    echo        winget install BrechtSanders.WinLibs.POSIX.UCRT
    exit /b 1
)

echo.
echo [1/4] Build memory-center-ffi dynamic library...
echo ----------------------------------------------------------------------------
cargo build --release -p memory-center-ffi
if errorlevel 1 (
    echo [ERROR] cargo build failed
    exit /b 1
)

echo.
echo [2/4] Copy sources to ASCII-only temp path...
echo ----------------------------------------------------------------------------
set "TMP_BUILD=%TEMP%\memory-center-go-build"
if exist "%TMP_BUILD%" rmdir /s /q "%TMP_BUILD%"
mkdir "%TMP_BUILD%"
if errorlevel 1 (
    echo [ERROR] Failed to create temp dir
    exit /b 1
)

if not exist "..\..\target\release\memory_center.dll" (
    echo [ERROR] ..\..\target\release\memory_center.dll not found
    exit /b 1
)
copy /Y "..\..\target\release\memory_center.dll" "%TMP_BUILD%\" >nul
copy /Y go.mod "%TMP_BUILD%\" >nul
copy /Y memorycenter.go "%TMP_BUILD%\" >nul
copy /Y MEMORY_CENTER_test.go "%TMP_BUILD%\" >nul
if errorlevel 1 (
    echo [ERROR] Failed to copy sources
    exit /b 1
)
echo Copied to: %TMP_BUILD%

echo.
echo [3/4] Generate mingw import lib (libmemory_center.dll.a)...
echo ----------------------------------------------------------------------------
pushd "%TMP_BUILD%"
if exist memory_center.def del memory_center.def
if exist libmemory_center.dll.a del libmemory_center.dll.a

gendef memory_center.dll
if not exist memory_center.def (
    echo [ERROR] gendef did not produce memory_center.def
    popd
    exit /b 1
)

dlltool -d memory_center.def -l libmemory_center.dll.a -k
if not exist libmemory_center.dll.a (
    echo [ERROR] dlltool did not produce libmemory_center.dll.a
    popd
    exit /b 1
)
echo Generated libmemory_center.dll.a

echo.
echo [4/4] Run Go tests...
echo ----------------------------------------------------------------------------
set "PATH=%CD%;%PATH%"
set "CGO_ENABLED=1"
set "CC=gcc"
go test -v ./...
if errorlevel 1 (
    echo.
    echo [ERROR] Go tests failed
    popd
    exit /b 1
)
popd

echo.
echo [OK] All tests passed
endlocal
exit /b 0
