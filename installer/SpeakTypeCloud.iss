; Inno Setup 7.1.0. No custom command execution or recursive data deletion.
#ifndef AppExe
  #error AppExe must be defined
#endif
#ifndef RepoRoot
  #error RepoRoot must be defined
#endif
#ifndef Version
  #error Version must be defined
#endif
#ifndef FixtureId
  #define ProductId "SpeakTypeCloud.Slint"
  #define ProductName "SpeakType Cloud"
  #define LegacyKey "Software\SpeakTypeCloud"
  #define LegacyUninstall "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakTypeCloud"
  #define StartupName "SpeakType Cloud"
  #define LaunchArgs ""
#else
  ; Isolated, clearly branded acceptance package; never published.
  #define ProductId "SpeakTypeCloud.Fixture." + FixtureId
  #define ProductName "SpeakType Cloud Fixture " + FixtureId
  #define LegacyKey "Software\SpeakTypeCloud.Fixture." + FixtureId
  #define LegacyUninstall "Software\Microsoft\Windows\CurrentVersion\Uninstall\SpeakTypeCloud.LegacyFixture." + FixtureId
  #define StartupName "SpeakType Cloud Fixture " + FixtureId
  #define LaunchArgs "--demo"
#endif

[Setup]
AppId={#ProductId}
AppName={#ProductName}
AppVersion={#Version}
AppPublisher=SpeakType Cloud
AppPublisherURL=https://github.com/stevenke1981/SpeakType-Cloud
DefaultDirName={localappdata}\Programs\SpeakType Cloud Slint
DefaultGroupName={#ProductName}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
AppMutex=Local\SpeakTypeCloud_SingleInstance
CloseApplications=no
RestartApplications=no
DisableProgramGroupPage=yes
DisableDirPage=no
DisableWelcomePage=no
UninstallDisplayIcon={app}\SpeakTypeCloud.exe
SetupIconFile={#RepoRoot}\dist\release\SpeakTypeCloud.ico
LicenseFile={#RepoRoot}\LICENSE
InfoBeforeFile={#RepoRoot}\installer\INSTALL-NOTES.txt
Compression=lzma2
SolidCompression=yes
OutputBaseFilename=SpeakTypeCloud-Setup-{#Version}
WizardStyle=modern
SetupLogging=yes

[Languages]
Name: "zh_TW"; MessagesFile: "compiler:Languages\ChineseTraditional.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "建立桌面捷徑"; GroupDescription: "捷徑："; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; DestName: "SpeakTypeCloud.exe"; Flags: ignoreversion
Source: "{#RepoRoot}\dist\release\SpeakTypeCloud.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\docs\licenses\slint-royalty-free-2.0.pdf"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{group}\{#ProductName}"; Filename: "{app}\SpeakTypeCloud.exe"; Parameters: "{#LaunchArgs}"
Name: "{group}\解除安裝 {#ProductName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#ProductName}"; Filename: "{app}\SpeakTypeCloud.exe"; Parameters: "{#LaunchArgs}"; Tasks: desktopicon

[Code]
const
  LegacyUninstallKey = '{#LegacyUninstall}';
  LegacyInstallKey = '{#LegacyKey}';
  OwnUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#ProductId}_is1';
  StartupKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  StartupValue = '{#StartupName}';

function SameDirectory(A, B: String): Boolean;
begin
  Result := CompareText(RemoveBackslashUnlessRoot(ExpandFileName(A)),
    RemoveBackslashUnlessRoot(ExpandFileName(B))) = 0;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  LegacyDir, OwnDir: String;
begin
  Result := '';
  if RegKeyExists(HKCU32, LegacyUninstallKey) or RegKeyExists(HKCU64, LegacyUninstallKey) then begin
    Result := '偵測到舊 NSIS 版本。請取消本次安裝，先從 Windows 已安裝的應用程式正常解除安裝舊版，再重新執行本安裝程式。舊版的設定、歷史、錄音和金鑰會保留；本安裝程式不會代為執行舊版解除安裝程式。';
    Exit;
  end;
  if RegQueryStringValue(HKCU32, LegacyInstallKey, 'InstallDir', LegacyDir) or
     RegQueryStringValue(HKCU64, LegacyInstallKey, 'InstallDir', LegacyDir) then
    if FileExists(AddBackslash(LegacyDir) + 'SpeakTypeCloud.exe') or
       FileExists(AddBackslash(LegacyDir) + 'Uninstall.exe') then begin
      Result := '舊版自訂安裝目錄仍有程式檔案。請先完成舊版正常解除安裝，勿將新版覆蓋在舊版目錄。';
      Exit;
    end;
  if FileExists(ExpandConstant('{app}\Uninstall.exe')) then begin
    Result := '所選資料夾仍包含舊版解除安裝程式，請選擇其他空資料夾。';
    Exit;
  end;
  if FileExists(ExpandConstant('{app}\SpeakTypeCloud.exe')) then begin
    if not RegQueryStringValue(HKCU, OwnUninstallKey, 'InstallLocation', OwnDir) or
       not SameDirectory(OwnDir, ExpandConstant('{app}')) then
      Result := '所選資料夾有不屬於此 Inno 安裝記錄的程式。請選擇空資料夾，避免覆寫。';
  end;
end;

function StartupMatchesInstalledApp: Boolean;
var
  Value, Expected: String;
begin
  Expected := '"' + ExpandConstant('{app}\SpeakTypeCloud.exe') + '"';
  Result := RegQueryStringValue(HKCU, StartupKey, StartupValue, Value) and
    ((CompareText(Value, Expected) = 0) or (CompareText(Value, Expected + ' --minimized') = 0));
end;

function InitializeUninstall: Boolean;
begin
  Result := not CheckForMutexes('Local\SpeakTypeCloud_SingleInstance');
  if not Result then
    MsgBox('請先從系統匣退出 SpeakType Cloud，再解除安裝。', mbError, MB_OK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    if StartupMatchesInstalledApp then
      RegDeleteValue(HKCU, StartupKey, StartupValue);
  { Inno removes only its logged application files and shortcuts.
    No configuration, history, recording or Credential Manager operations. }
end;
