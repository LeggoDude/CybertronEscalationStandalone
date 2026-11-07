; Cybertron Escalation Setup: the installer for players (Inno Setup 6.3 or newer).
; tools\make_installer.py ("make installer.bat") builds what it installs into
; installer_work\stage and compiles this with it:
;   ISCC /DStage=<stage> /DAppVersion=<version> /DOutputDir=<folder> foc_esc.iss
;
; It installs, for the player alone (no administrator needed), into
; %LOCALAPPDATA%\Programs\Cybertron Escalation: the demo (foc_esc.exe, built
; ready to run), its own shaders, the asset build's tools (tools\, with UE Viewer)
; and a Python of their own to run them (python\). The game files aren't in it: as
; Setup finishes, the builder (tools\installer\builder.py) builds them from the
; player's own copy of Fall of Cybertron. The demo runs the builder itself while
; they aren't built, or an update needs them built again.

#ifndef Stage
  #error Run tools\make_installer.py: it builds the files to install and passes their folder (/DStage=...)
#endif
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\installer_out"
#endif

#define AppName "Cybertron Escalation Standalone"
#define ShortName "Cybertron Escalation"

[Setup]
; (The same id for every version, so a new one installs over the last.)
AppId={{7D3B2F4E-6A51-4C3B-9E0F-2B8D7C1A5E93}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} Setup
DefaultDirName={localappdata}\Programs\{#ShortName}
DefaultGroupName={#ShortName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename={#ShortName} Setup {#AppVersion}
SetupIconFile={#Stage}\icon\foc_esc.ico
UninstallDisplayIcon={app}\foc_esc.exe
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; (The game files, built after: about 2.4 GB, and as much again while they build.)
ExtraDiskSpaceRequired=6442450944
CloseApplications=yes
RestartApplications=no
DirExistsWarning=no

[Messages]
WelcomeLabel2=This installs [name/ver] on your computer.%n%nYou need Transformers: Fall of Cybertron installed (from Steam): the demo builds its game files from your own copy. When Setup finishes, that takes about an hour and needs about 6 GB of free space. You can use your computer meanwhile.
FinishedHeadingLabel=Almost there
FinishedLabelNoIcons=Setup has installed [name]. Next, it builds its game files from your copy of Fall of Cybertron (about an hour). If you skip that now, it happens the first time you play.
FinishedLabel=Setup has installed [name]. Next, it builds its game files from your copy of Fall of Cybertron (about an hour). If you skip that now, it happens the first time you play.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[InstallDelete]
; (Replaced whole by each version: its Python and the build's tools. The ffmpeg the
; builder downloaded, tools\ffmpeg, is kept.)
Type: filesandordirs; Name: "{app}\python"
Type: filesandordirs; Name: "{app}\tools\installer"
Type: filesandordirs; Name: "{app}\tools\umodel"
Type: filesandordirs; Name: "{app}\tools\ue3"
Type: filesandordirs; Name: "{app}\tools\__pycache__"
Type: files; Name: "{app}\tools\*.py"

[Files]
Source: "{#Stage}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#ShortName}"; Filename: "{app}\foc_esc.exe"; WorkingDir: "{app}"
Name: "{group}\Build the game files"; Filename: "{app}\python\pythonw.exe"; Parameters: "-E -s ""{app}\tools\installer\builder.py"""; WorkingDir: "{app}"; IconFilename: "{app}\foc_esc.exe"; Comment: "Builds the demo's game files from your copy of Fall of Cybertron"
Name: "{autodesktop}\{#ShortName}"; Filename: "{app}\foc_esc.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\python\pythonw.exe"; Parameters: "-E -s ""{app}\tools\installer\builder.py"" --after-setup"; WorkingDir: "{app}"; Description: "Build the game files now (about an hour)"; Flags: postinstall nowait

[UninstallDelete]
; (What the demo and the builder made: the game files, the build's work files,
; the downloaded ffmpeg, Python's caches, the logs.)
Type: filesandordirs; Name: "{app}\assets"
Type: filesandordirs; Name: "{app}\extract_work"
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\python"
Type: files; Name: "{app}\game_files.txt"
Type: files; Name: "{app}\game_folder.txt"
Type: files; Name: "{app}\crash.txt"
Type: files; Name: "{app}\netlog.txt"
Type: dirifempty; Name: "{app}"

[Code]
{ The player's own files (progress, Customize builds, settings) stay unless they
  say to remove them too. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Progress, Builds, Settings: String;
begin
  if CurUninstallStep <> usUninstall then
    exit;
  Progress := ExpandConstant('{app}\progress.txt');
  Builds := ExpandConstant('{app}\builds.txt');
  Settings := ExpandConstant('{app}\settings.txt');
  if not (FileExists(Progress) or FileExists(Builds) or FileExists(Settings)) then
    exit;
  if UninstallSilent then
    exit;
  if MsgBox('Also remove your progress (levels, shards and parts bought), your Customize builds and your settings?'
      + #13#10#13#10 + 'Choose No to keep them for the next time you install the demo.',
      mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
  begin
    DeleteFile(Progress);
    DeleteFile(Builds);
    DeleteFile(Settings);
  end;
end;
