#define MyAppName "汲湍 (JiTuan)"
#define MyAppVersion "0.9.23"
#define MyAppPublisher "choked-up-now"
#define MyAppURL "https://atomgit.com/choked-up-now/JTWin"

[Setup]
AppId={{D3C9F88A-7F1B-4C1A-9B2A-8A2C5F6D7E8B}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\JiTuan
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir=Output
OutputBaseFilename=JiTuan_v{#MyAppVersion}_Setup
Compression=lzma
SolidCompression=yes
WizardStyle=polar
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加图标:"
Name: "internal_editor"; Description: "使用汲湍内置的 JSON 编辑器 (推荐)"; GroupDescription: "镜像规则编辑器偏好:"; Flags: checkedonce

[Components]
Name: "gui"; Description: "汲湍 图形界面版 (GUI)"; Types: full compact custom
Name: "cli"; Description: "汲湍 命令行版 (CLI, 将自动加入系统 PATH)"; Types: full custom
Name: "p2p"; Description: "安装 P2P 种子/磁力链下载支持 (增加约 1.5MB)"; Types: full; Flags: checkablealone

[Files]
Source: "JiTuan_Release\jituan-gui-full.exe"; DestDir: "{app}"; DestName: "jituan-gui.exe"; Components: gui and p2p
Source: "JiTuan_Release\jituan-gui-lite.exe"; DestDir: "{app}"; DestName: "jituan-gui.exe"; Components: gui and not p2p

Source: "JiTuan_Release\jituan-cli-full.exe"; DestDir: "{app}"; DestName: "jituan-cli.exe"; Components: cli and p2p
Source: "JiTuan_Release\jituan-cli-lite.exe"; DestDir: "{app}"; DestName: "jituan-cli.exe"; Components: cli and not p2p

Source: "JiTuan_Release\LICENSE"; DestDir: "{app}"; Components: gui or cli
Source: "JiTuan_Release\NOTICE"; DestDir: "{app}"; Components: gui or cli

[Icons]
Name: "{group}\汲湍"; Filename: "{app}\jituan-gui.exe"; Components: gui
Name: "{group}\卸载 汲湍"; Filename: "{uninstallexe}"
Name: "{autodesktop}\汲湍"; Filename: "{app}\jituan-gui.exe"; Tasks: desktopicon; Components: gui

[Run]
Filename: "{app}\jituan-gui.exe"; Description: "立即运行 汲湍"; Flags: nowait postinstall skipifsilent; Components: gui

[Code]
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if CurPageID = wpSelectComponents then
  begin
    if (not WizardIsComponentSelected('gui')) and (not WizardIsComponentSelected('cli')) then
    begin
      MsgBox('必须至少安装一个界面组件（GUI 或 CLI）！', mbError, MB_OK);
      Result := False;
    end;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  OldPath: string;
  NewPath: string;
  AppDataDir: string;
  FlagFile: string;
begin
  if CurStep = ssPostInstall then
  begin
    if WizardIsComponentSelected('cli') then
    begin
      if RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', OldPath) then
      begin
        if Pos(ExpandConstant('{app}'), OldPath) = 0 then
        begin
          NewPath := OldPath + ';' + ExpandConstant('{app}');
          RegWriteExpandStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', NewPath);
        end;
      end;
    end;

    AppDataDir := ExpandConstant('{userappdata}\JiTuan');
    if not DirExists(AppDataDir) then
      ForceDirectories(AppDataDir);
      
    FlagFile := AppDataDir + '\use_internal_editor.flag';
    
    if WizardIsTaskSelected('internal_editor') then
    begin
      SaveStringToFile(FlagFile, '1', False);
    end
    else
    begin
      if FileExists(FlagFile) then
        DeleteFile(FlagFile);
    end;
  end;
end;