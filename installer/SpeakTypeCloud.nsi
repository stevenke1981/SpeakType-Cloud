Unicode true
RequestExecutionLevel user
SetShellVarContext current

!include "MUI2.nsh"

!ifndef APP_EXE
  !error "APP_EXE must point to SpeakTypeCloud.exe"
!endif
!ifndef ICON_FILE
  !error "ICON_FILE must point to SpeakTypeCloud.ico"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "SpeakTypeCloud-Setup.exe"
!endif
!ifndef VERSION
  !define VERSION "0.0.0"
!endif

!define APP_NAME "SpeakType Cloud"
!define COMPANY_NAME "SpeakType Cloud"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakTypeCloud"

Name "${APP_NAME}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\SpeakType Cloud"
InstallDirRegKey HKCU "Software\SpeakTypeCloud" "InstallDir"
Icon "${ICON_FILE}"
UninstallIcon "${ICON_FILE}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "TradChinese"
!insertmacro MUI_LANGUAGE "English"

Section "Install" SEC_MAIN
  SetOutPath "$INSTDIR"
  File /oname=SpeakTypeCloud.exe "${APP_EXE}"
  File /oname=SpeakTypeCloud.ico "${ICON_FILE}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\SpeakType Cloud"
  CreateShortcut "$SMPROGRAMS\SpeakType Cloud\SpeakType Cloud.lnk" "$INSTDIR\SpeakTypeCloud.exe" "" "$INSTDIR\SpeakTypeCloud.ico"
  CreateShortcut "$SMPROGRAMS\SpeakType Cloud\Uninstall SpeakType Cloud.lnk" "$INSTDIR\Uninstall.exe"

  WriteRegStr HKCU "Software\SpeakTypeCloud" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${COMPANY_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\SpeakTypeCloud.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$"$INSTDIR\Uninstall.exe$"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '$"$INSTDIR\Uninstall.exe$" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\SpeakType Cloud\SpeakType Cloud.lnk"
  Delete "$SMPROGRAMS\SpeakType Cloud\Uninstall SpeakType Cloud.lnk"
  RMDir "$SMPROGRAMS\SpeakType Cloud"
  Delete "$INSTDIR\SpeakTypeCloud.exe"
  Delete "$INSTDIR\SpeakTypeCloud.ico"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "SpeakType Cloud"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "Software\SpeakTypeCloud"
  ; User configuration, history, recordings, and Credential Manager entries are preserved.
SectionEnd
