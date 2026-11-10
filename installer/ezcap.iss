#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\ezcap.exe"
#endif
#ifndef ThirdParty
  #define ThirdParty "..\dist\THIRD-PARTY.txt"
#endif

[Setup]
AppId={{A7C3E1B2-4D5F-4A89-9C0E-1B2D3E4F5A60}
AppName=ezcap
AppVersion={#AppVersion}
AppPublisher=ezcap
AppPublisherURL=https://github.com/machine202502/ezcap
AppSupportURL=https://github.com/machine202502/ezcap
AppUpdatesURL=https://github.com/machine202502/ezcap/releases
DefaultDirName={autopf}\ezcap
DefaultGroupName=ezcap
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=ezcap-{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\ezcap.ico
UninstallDisplayIcon={app}\ezcap.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=yes
InfoBeforeFile=..\NOTICE

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "ezcap.exe"; Flags: ignoreversion
Source: "..\COPYING"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\locales\*.json"; DestDir: "{app}\locales"; Flags: ignoreversion
Source: "{#ThirdParty}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\ezcap"; Filename: "{app}\ezcap.exe"
Name: "{autodesktop}\ezcap"; Filename: "{app}\ezcap.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ezcap.exe"; Description: "Launch ezcap"; Flags: nowait postinstall skipifsilent
