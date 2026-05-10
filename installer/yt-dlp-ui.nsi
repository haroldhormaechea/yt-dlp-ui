; yt-dlp-ui.nsi — NSIS installer script for Windows.
;
; Per-user install at $LOCALAPPDATA\Programs\yt-dlp-ui (modern Windows
; convention used by GitHub Desktop, Discord, etc.; no UAC prompt;
; non-technical-user friendly per UC 01 audience).
;
; 64-bit Unicode target.
;
; Driven by package-nsis.yml — variables expected to be defined via
; /D switches at the makensis command line:
;   /DPRODUCT_VERSION=<semver>
;   /DSOURCE_DIR=<path containing yt-dlp-ui.exe, ad-window.exe, yt-dlp, deno>
;   /DWEBVIEW2_BOOTSTRAPPER=<absolute path to MicrosoftEdgeWebview2Setup.exe>
;
; Output: yt-dlp-ui-installer.exe in the working directory.

Unicode true
; x86-unicode produces a 32-bit NSIS stub that can install 64-bit application
; binaries without issues. The amd64-unicode stub requires additional NSIS
; stub files that the chocolatey nsis package does not ship. If a native 64-bit
; installer is required in future, replace choco install nsis with the official
; NSIS installer (which bundles all stubs) and restore Target amd64-unicode.
Target x86-unicode

!ifndef PRODUCT_VERSION
    !define PRODUCT_VERSION "0.0.0"
!endif

; VIProductVersion requires the strict X.X.X.X (four-integer) format that
; Windows resource metadata expects. Pre-release tags like "0.5.1-rc.2" are
; not valid there, so the workflow pre-processes the semver and passes the
; sanitised four-part string via /DPRODUCT_VI_VERSION. Fallback is "0.0.0.0".
!ifndef PRODUCT_VI_VERSION
    !define PRODUCT_VI_VERSION "0.0.0.0"
!endif

!ifndef SOURCE_DIR
    !define SOURCE_DIR "."
!endif

!ifndef WEBVIEW2_BOOTSTRAPPER
    !error "WEBVIEW2_BOOTSTRAPPER not defined; pass /DWEBVIEW2_BOOTSTRAPPER=path"
!endif

!define PRODUCT_NAME      "yt-dlp-ui"
!define PRODUCT_PUBLISHER "Harold Hormaechea"
!define PRODUCT_WEB_SITE  "https://github.com/HaroldHormaechea/yt-dlp-ui"

Name              "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile           "yt-dlp-ui-installer.exe"
InstallDir        "$LOCALAPPDATA\Programs\yt-dlp-ui"
RequestExecutionLevel user
ShowInstDetails   show
ShowUnInstDetails show

; Modern UI 2.
!include "MUI2.nsh"

!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE_DIR}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

VIProductVersion "${PRODUCT_VI_VERSION}"
VIAddVersionKey  "ProductName"     "${PRODUCT_NAME}"
VIAddVersionKey  "CompanyName"     "${PRODUCT_PUBLISHER}"
VIAddVersionKey  "FileVersion"     "${PRODUCT_VERSION}"
VIAddVersionKey  "ProductVersion"  "${PRODUCT_VERSION}"
VIAddVersionKey  "FileDescription" "${PRODUCT_NAME} ${PRODUCT_VERSION}"

Section "MainSection" SEC01
    SetOutPath "$INSTDIR"
    SetOverwrite on

    File "${SOURCE_DIR}\yt-dlp-ui.exe"
    File "${SOURCE_DIR}\ad-window.exe"
    ; runtime-deps deliver canonical-name binaries (no extension) — see
    ; UC 06 Smoke 1. paths.rs Windows branch probes <bin>.exe first then
    ; <bin> (no extension). Install both names to avoid breakage when an
    ; admin manually copies a .exe into place.
    File "${SOURCE_DIR}\yt-dlp"
    File "${SOURCE_DIR}\deno"
    ; UC 17 — bundled LGPL-only ffmpeg + LICENSE text. Same canonical-
    ; name posture as yt-dlp / deno.
    File "${SOURCE_DIR}\ffmpeg"
    File "${SOURCE_DIR}\ffmpeg-LICENSE.txt"
    File "${SOURCE_DIR}\yt-dlp-LICENSE.txt"
    File "${SOURCE_DIR}\LICENSE"

    ; Bundle the WebView2 Evergreen Bootstrapper inside the installer.
    ; Auto-installs the WebView2 runtime at first launch then deletes
    ; itself. Without it, Win10 installs without Edge get a dud install.
    File "/oname=MicrosoftEdgeWebview2Setup.exe" "${WEBVIEW2_BOOTSTRAPPER}"
    DetailPrint "Installing WebView2 Runtime if needed..."
    ExecWait '"$INSTDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install' $0
    ${If} $0 != 0
        DetailPrint "WebView2 install returned $0 (non-fatal; user may need to install manually)."
    ${EndIf}
    Delete "$INSTDIR\MicrosoftEdgeWebview2Setup.exe"

    ; Start menu shortcut.
    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortCut  "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\yt-dlp-ui.exe"

    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Add/Remove Programs entry (per-user hive: HKCU).
    !define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayName"     "${PRODUCT_NAME}"
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion"  "${PRODUCT_VERSION}"
    WriteRegStr HKCU "${UNINST_KEY}" "Publisher"       "${PRODUCT_PUBLISHER}"
    WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout"    "${PRODUCT_WEB_SITE}"
    WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\yt-dlp-ui.exe"
    Delete "$INSTDIR\ad-window.exe"
    Delete "$INSTDIR\yt-dlp"
    Delete "$INSTDIR\deno"
    Delete "$INSTDIR\ffmpeg"
    Delete "$INSTDIR\ffmpeg-LICENSE.txt"
    Delete "$INSTDIR\yt-dlp-LICENSE.txt"
    Delete "$INSTDIR\LICENSE"
    Delete "$INSTDIR\Uninstall.exe"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    RMDir  "$SMPROGRAMS\${PRODUCT_NAME}"
    RMDir  "$INSTDIR"

    DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
