"""Builds the demo's main menu from Fall of Cybertron's own front end.

usage: python build_menu.py <TransGame folder> <umodel Texture2D folder> <foc_esc/assets folder>
                             [<umodel Texture2D folder of mp_shared.xxx>
                              [<umodel Texture2D folder of UI_Lobby_ESC_m.xxx>]]
       python build_menu.py --text <TransGame folder> <foc_esc/assets folder>
           rewrites only fonts/*.ttf and menu.txt (no images or movies)

The game's main menu is the Scaleform movie UI_GFxFrontEnd_p.FrontEnd_GFX (its
menu_Main clip), with the settings menu UI_GFxSettings_p.SettingsMenu_GFX; both are
cooked into CookedPC/UI_FrontEnd_m.xxx. Behind the menu the game plays the Bink
movie Movies/BG_RustSea.bik. The Extras menu (UI_GFxExtras_p.ExtrasMenu_GFX, in
UI_FrontEnd_m.xxx too) plays the ending credits, the Bink movie
Movies/TFOC_Ending_Credits.bik. This script writes assets/menu/:

  background.h264     BG_RustSea.bik re-encoded as H.264 (Baseline profile, which
                      the demo's decoder reads), looping seamlessly as in the game
  credits.h264        TFOC_Ending_Credits.bik's picture, the same way
  credits.ogg         its sound (the movie's first two tracks, left and right;
                      its other four are silent) as Ogg Vorbis
  img/*.png           the menu's images (logo, selection brackets, settings menu
                      art, slider parts, button icons, the still background, the
                      Escalation lobby's art and icons)
  fonts/*.ttf         the movie fonts it uses ($NormalFont = Register Demi Bold,
                      $TitleFont = Register Medium, $HeaderFont = Register
                      Light, and the Customize lists' prices' $NumberFontWide =
                      Energon Numbers Wide), rebuilt as TrueType from
                      UI_GFxFonts_p.Fonts_EFIGS_GFX in CookedPC/Startup_int.xxx
                      (named without their spaces)
  menu.txt            the menu's text in English, read from the game's
                      Localization/Cooked/PC/Coalesced.int (UIText.int), and the
                      fonts' advance widths, one record per line:

    string <Section.Key> <text>
    advance <font> <character code> <advance in ems>

The images come from umodel's export of UI_FrontEnd_m.xxx (run it first):

  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> UI_FrontEnd_m.xxx

and pass <dir>/UI_FrontEnd_m/Texture2D as the second argument.

The in-game pause menu (UI_GFxPause_p.PauseMenu_GFX) brings two images of its own,
the Teletraan 1 logo and emblem, which are cooked into mp_shared.xxx; export them
too and pass that export's Texture2D folder as the fourth argument:

  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PauseMenu_GFX_I51 Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PauseMenu_GFX_I4D Texture2D

The Escalation lobby (UI_GFxLobbies_p.MPLobby_GFX, with its player list
UI_GFxPlayerList_p.PlayerList_GFX) brings its own art too. The player list's
connection bars are in mp_shared.xxx (export them into the same folder as the
pause menu's images):

  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PlayerList_GFX_I51 Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PlayerList_GFX_I53 Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PlayerList_GFX_I55 Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PlayerList_GFX_I27 Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx PlayerList_GFX_I2B Texture2D

and the lobby's own images are in UI_Lobby_ESC_m.xxx; export it whole and pass
<dir>/UI_Lobby_ESC_m/Texture2D as the fifth argument:

  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> UI_Lobby_ESC_m.xxx

The lobby's icons (the Escalation mode icon, the selector arrows, the prompts'
icons) are drawn as vector shapes in the movies; they're rasterized from the
movies themselves (in UI_FrontEnd_m.xxx and UI_GFxShared_p.xxx).

Needs ffmpeg (with libx264) on the PATH and the fontTools and Pillow Python
packages.
"""
import math
import os
import shutil
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import gfx_io  # noqa: E402
import gfx_render  # noqa: E402
from build_hud import swf_movie  # noqa: E402

# Images the menu uses: (file written, file in the umodel export, what it is).
IMAGES = [
    ("background_still.png", "FrontEnd_GFX_I220", "the menu movie's still background (shown until the movie runs)"),
    ("logo.png", "FrontEnd_GFX_I193", "TRANSFORMERS: FALL OF CYBERTRON logo (logo_mc, INT)"),
    ("brackets.png", "FrontEnd_GFX_I16C", "MainMenuButton's selection brackets"),
    ("settings_background.png", "MenuBackground", "SharedAssets mc_menuBackgroundArt"),
    ("selection_bar.png", "SelectionBar", "SharedAssets mc_selectionBarArt"),
    ("extras_selection_bar.png", "ExtrasMenu_GFX_I2F", "the Extras menu's selection bar (its sprite 51, 'normal')"),
    ("slider_track.png", "SettingsMenu_GFX_I15", "MenuSlider_track"),
    ("slider_bar.png", "SettingsMenu_GFX_I19", "MenuSlider_bar"),
    ("slider_arrow.png", "SettingsMenu_GFX_I1D", "MenuSlider_arrow"),
    # Settings > Display's Brightness and Safe Frame screens (UI_GFxSettings_p's
    # Brightness_GFX, CalibrationImage_GFX and SafeFrame_GFX).
    ("brightness_panel.png", "Brightness_GFX_I12", "the Brightness screen's glowing panel (its sprite 20)"),
    ("calibration.png", "CalibrationImage_CLR", "the brightness calibration picture"),
    ("brightness_track.png", "Brightness_GFX_IC", "TransSlider_track"),
    ("brightness_bar.png", "Brightness_GFX_I8", "TransSlider_bar"),
    ("brightness_arrow.png", "Brightness_GFX_I4", "TransSlider_arrow"),
    ("safe_frame_area.png", "SafeFrame_GFX_I4", "the Safe Frame screen's safe area (safeArea_mc)"),
    ("safe_frame_arrow.png", "SafeFrame_GFX_I7", "its arrows (upArrow_mc...)"),
    ("safe_frame_band.png", "SafeFrame_GFX_IC", "the band behind its directions (its shape 13)"),
    ("button_a.png", "Gamepad_A", "button hint icon"),
    ("button_b.png", "Gamepad_B", "button hint icon"),
    # The pause menu's shell: SharedAssets art (in the front end's export)...
    ("pause_hexagons.png", "WarpedHexagonBackground", "SharedAssets mc_warpedHexagonBackgroundArt"),
    ("pause_emblem_glow.png", "TeletranEmblemGlow", "SharedAssets mc_teletranEmblemGlowArt"),
    ("pause_pip.png", "LineSegment", "SharedAssets mc_lineSegmentArt (pips_mc)"),
]
# ... and its own images (in mp_shared.xxx's export: the fourth argument).
PAUSE_IMAGES = [
    ("pause_logo.png", "PauseMenu_GFX_I51", "TELETRAAN 1 logo (logo_mc)"),
    ("pause_emblem.png", "PauseMenu_GFX_I4D", "Teletraan emblem (emblem_mc)"),
    # The player list's connection bars (PlayerList_GFX, also in mp_shared.xxx).
    ("lobby_ping_1.png", "PlayerList_GFX_I51", "connection: poor (one red bar)"),
    ("lobby_ping_2.png", "PlayerList_GFX_I53", "connection: fair (two yellow bars)"),
    ("lobby_ping_3.png", "PlayerList_GFX_I55", "connection: good (three green bars)"),
    # The Prime medal by a Prime's name (PlayerList_GFX's mc_awardPrimeMode): its
    # star, turning, and its ring, flipping.
    ("lobby_prime_star.png", "PlayerList_GFX_I27", "the Prime medal's star"),
    ("lobby_prime_ring.png", "PlayerList_GFX_I2B", "the Prime medal's ring"),
]
# The Escalation lobby's images (MPLobby_GFX, in UI_Lobby_ESC_m.xxx's export: the
# fifth argument). The textures are stretched to powers of two; the demo draws them
# at the sizes the movie gives them.
LOBBY_IMAGES = [
    ("lobby_clouds.png", "MPLobby_GFX_IC9", "clouds_bg's cloud tile"),
    ("lobby_panels.png", "MPLobby_GFX_I38", "the lobby's panels"),
    ("lobby_bar.png", "MPLobby_GFX_I19", "MenuButton's selection bar"),
    ("lobby_bar_selector.png", "MPLobby_GFX_I1B", "the selection bar on a selector (< EASY >)"),
    ("lobby_map_frame.png", "mapFrame", "mc_mapImage's frame"),
    ("lobby_map_shade.png", "MPLobby_GFX_I52", "the shade under the map's name"),
    ("lobby_lock.png", "MPLobby_GFX_I32", "the lock on the Prime Mode row (mc_lockPanelButton's mc_lock)"),
    # The maps' pictures (main.rs's MAPS): Downfall is ESC_Iacon_BASE_m, MapId 1603
    # in TransLevels.ini, whose picture is mapImg1603; Oblivion is ESC_Ark_BASE_m,
    # MapId 1600; Ancients is ESC_Temple_BASE_m, MapId 1602; Ignition is
    # ESC_Gladiator_BASE_m, MapId 1601.
    ("lobby_map_downfall.png", "mapImg1603", "Downfall's picture"),
    ("lobby_map_oblivion.png", "mapImg1600", "Oblivion's picture"),
    ("lobby_map_ancients.png", "mapImg1602", "Ancients' picture"),
    ("lobby_map_ignition.png", "mapImg1601", "Ignition's picture"),
    ("lobby_spinner.png", "MPLobby_GFX_I14", "mc_playerConnectingIcon"),
]
# Vector art, rasterized from the movies: (file written, package, movie, character
# (an export name or a character id), pixels per movie pixel).
VECTOR_IMAGES = [
    ("lobby_mode_icon.png", "UI_FrontEnd_m.xxx", "UI_GFxShared_p.SharedIcons_GFX",
     "mc_gameModeIconSV", 8.0),
    ("lobby_arrow.png", "UI_FrontEnd_m.xxx", "UI_GFxShared_p.SharedComponents_GFX", 37, 3.0),
    ("prompt_icon.png", "UI_GFxShared_p.xxx", "TextPrompt_GFX", 6, 2.0),
    ("prompt_alert.png", "UI_GFxShared_p.xxx", "MessagePrompt_GFX", 27, 2.0),
]

# Text the menu shows (UIText.int keys, as FrontEnd_GFX's labelIDs and
# btnDescArray name them).
STRINGS = [
    "Lobby.Customize", "Lobby.CustomizeDesc",  # in place of MULTIPLAYER
    "MainMenu.Escalation", "MainMenu.EscalationDesc",
    "Shared.Settings", "MainMenu.SettingsDesc",
    "Extras.ExtrasTitle", "MainMenu.ExtrasDesc",
    "MainMenu.ExitGame", "MainMenu.ExitDesc",
    "ButtonHints.Accept", "ButtonHints.Back",
    "Settings.Audio",
    # The settings menus (SettingsMenu_GFX's RootMenuProvider, Graphics' Fullscreen
    # Mode and its On / Off values).
    "Settings.Display", "Settings.Graphics", "Settings.Controls",
    "Settings.Fullscreen", "Settings.On", "Settings.Off",
    # The Graphics menu (GraphicsMenuProvider): its Graphics row's values
    # (GraphicsSettings), Resolution and VSync.
    "Settings.Low", "Settings.Medium", "Settings.High", "Settings.Extra",
    "Settings.Resolution", "Settings.VSync",
    # The Display menu (DisplayMenuProvider): Brightness and Safe Frame (their
    # screens' directions, and Accept) and Team Coloring Mode with its two values.
    "Settings.Brightness", "Settings.BrightnessDirections", "Settings.SafeFrame",
    "Settings.SafeFrameDirections", "Settings.TeamColorMode", "Settings.Accept",
    "Shared.Ally", "Shared.Versus", "Shared.Enemy", "Shared.Autobot", "Shared.Decepticon",
    # The Audio menu's volumes (AudioMenuProvider's sliders).
    "Settings.EffectsVolume", "Settings.DialogVolume", "Settings.MusicVolume",
    # The pause menu (MultiplayerMenuProvider, its title), and Quit Game's question
    # (TnQuitMessageBox: its title and buttons).
    "Pause.Paused", "Pause.ResumeGame", "Pause.ChooseCharacter", "Pause.QuitGame",
    "MessagePrompts.LeaveLobby", "ButtonHints.Yes", "ButtonHints.No",
    # The Extras menu (ExtrasMenu_GFX's mc_rootMenu: its title, and the Credits
    # button with its rollover hint).
    "Extras.ExtrasTitle", "Extras.CreditsButton", "Extras.CreditsRollover",
    # The Escalation lobby (MPLobby_GFX's party lobby and game lobby, the player
    # list's heading, Connecting to Host, the countdown).
    "Lobby.EscalationLobbyLabel", "Lobby.PartyLobbyTitle", "Lobby.PartyMembersHeader",
    "OnlineFriends.JoinGame", "Lobby.HostGame", "Lobby.HostDesc",
    "Leaderboards.Leaderboards", "Lobby.LeaderboardDesc",
    "Lobby.InviteFriends", "Lobby.InviteFriendsDesc", "OnlineFriends.InviteDesc",
    "Lobby.StartGame", "Lobby.StartGameDesc", "Lobby.SelectMap", "Lobby.SelectMapDesc",
    "Difficulty.Difficulty", "Difficulty.Easy", "Difficulty.Medium", "Difficulty.Hard",
    "Lobby.ChooseDifficultyDesc", "Lobby.ConnectingToHost", "Lobby.MultiplayerCountdownLabel",
    # Prime Mode (the multiplayer lobby's mc_menuTeletran1 has it; the demo's party
    # lobby has it as a row): its button, its hints locked and open, and its
    # question with Confirm and Cancel.
    "Lobby.PrimeMode", "Lobby.PrimeModeDesc", "Lobby.PrimeModeUnlockedDesc",
    "MessagePrompts.PrimeModeMessage", "ButtonHints.Confirm",
    # The scoreboard in a match (InGameStats_GFX's gameModeOnMap_txt).
    "Shared.GameModeOnMap",
    # Prompts (TextPrompt_GFX, MessagePrompt_GFX) and their messages.
    "MessagePrompts.JoinFriendGameJoinButton", "MessagePrompts.JoinGameInviteFailedTitle",
    "MessagePrompts.JoinGameInviteFailedMessage", "MessagePrompts.StrictNATTitle",
    "ButtonHints.Cancel", "ButtonHints.Ok", "Errors.FailedToConnect", "Errors.HostQuitError",
    # Settings > Controls (ControlsMenuProvider, as the PC version lists it: the
    # LS / RS Button Delay under its Xbox 360 name) and its values.
    "Settings.ActiveControls", "Settings.KeyboardMouseLayout", "Settings.ControllerLayout",
    "Settings.ControllerVibration", "Settings.VehicleBoost", "Settings.HeavyThumbXBOX",
    "Settings.RobotControl", "Settings.GroundVehicleControl", "Settings.AirVehicleControl",
    "Settings.CameraSensitivity", "Settings.MouseSensitivity",
    "Settings.KeyboardMouse", "Settings.Controller", "Settings.Hold", "Settings.Toggle",
    "Settings.Normal", "Settings.Inverted",
    # The Controller Layout screen (ControllerLayoutMenuProvider): its Thumbsticks
    # row, and what it says with no controller (mc_controllerMapNone).
    "Settings.StickLayout", "MessagePrompts.ControllerErrorTitle",
    # The Keyboard/Mouse Layout screen (KeyboardMouseLayoutMenuProvider): Select
    # Layout, View Layout, and its tables' actions and keys (mc_keyboardMouse).
    "Settings.SelectLayout", "Settings.ViewLayout", "Settings.Default",
    "Settings.RobotControls", "Settings.VehicleControls", "Settings.GrimlockControls",
    "KeyAction.Move", "KeyAction.TurnLook", "KeyAction.TurnSteer", "KeyAction.SwapWeapons",
    "KeyAction.Fire", "KeyAction.Melee", "KeyAction.FineAim", "KeyAction.DashSprint",
    "KeyAction.Jump", "KeyAction.JumpAscend", "KeyAction.Descend", "KeyAction.SpeedBoost",
    "KeyAction.Ability", "KeyAction.Interact", "KeyAction.ChangeForm", "KeyAction.ChangeHand",
    "KeyAction.Reload", "KeyAction.Pause", "KeyAction.OffensiveEq", "KeyAction.DefensiveEq",
    "KeyAction.Scoreboard", "KeyAction.ThrowGrenade", "KeyAction.GrabFire", "KeyAction.Execute",
    "KeyAction.Block",
    "Keyboard.Move", "Keyboard.MoveSouthPaw", "Keyboard.Mouse", "Keyboard.Wheel",
    "Keyboard.MouseWheelUp", "Keyboard.LeftMouseButton", "Keyboard.RightMouseButton",
    "Keyboard.MiddleMouseButton", "Keyboard.ShiftKey", "Keyboard.Control", "Keyboard.Space",
    "Keyboard.Tab", "Keyboard.Escape", "Keyboard.Semicolon", "Keyboard.ForwardSlash",
    "Keyboard.BKey", "Keyboard.CKey", "Keyboard.EKey", "Keyboard.FKey", "Keyboard.GKey",
    "Keyboard.HKey", "Keyboard.NKey", "Keyboard.OKey", "Keyboard.QKey", "Keyboard.RKey",
    "Keyboard.TKey", "Keyboard.UKey", "Keyboard.VKey", "Keyboard.XKey", "Keyboard.YKey",
    "Keyboard.Num0Key", "Keyboard.Num1Key", "Keyboard.Num2Key", "Keyboard.Num5Key",
    "Keyboard.Num9Key",
    # The starting scoreboard (PreGameCountdown_GFX's waiting_mc).
    "MessagePrompts.WaitingForPlayers",
    # The notice for a damaged profile (TnPlayerController.
    # OnReadProfileSettingsComplete's popup), shown for a progress.txt changed by
    # hand (src/xp.rs).
    "MessagePrompts.GenericErrorTitle", "MessagePrompts.ResetCorruptProfile",
    # The main menu's Exit Game question (its title, question and buttons).
    "MessagePrompts.GenericWarningTitle", "MessagePrompts.ExitGameMessage",
    "MessagePrompts.GenericContinueButton", "MessagePrompts.GenericCancelButton",
]
# Text from the game's other localization files: (file in Coalesced.int, its
# Section.Key, the key menu.txt gives it).
OTHER_STRINGS = [
    # The PC version's notice that the Graphics row's texture quality waits for a
    # restart (UIText_PC.int's message prompt).
    ("TransGame/Localization/INT/UIText_PC.int", "MessagePrompts.RestartToApplyTextureSettingsTitle",
     "MessagePrompts.RestartToApplyTextureSettingsTitle"),
    ("TransGame/Localization/INT/UIText_PC.int", "MessagePrompts.RestartToApplyTextureSettingsMessage",
     "MessagePrompts.RestartToApplyTextureSettingsMessage"),
    ("TransGame/Localization/INT/Engine.int", "Errors.ConnectionFailed_Title",
     "Engine.Errors.ConnectionFailed_Title"),
    ("TransGame/Localization/INT/Engine.int", "Errors.ClientOutdated", "Engine.Errors.ClientOutdated"),
    # Game Password (Host a Game's prompt, and a game with a password asking for it).
    ("TransGame/Localization/INT/Engine.int", "AccessControl.ACDisplayText[0]",
     "Engine.AccessControl.GamePassword"),
    ("TransGame/Localization/INT/Engine.int", "AccessControl.ACDescText[0]",
     "Engine.AccessControl.GamePasswordDesc"),
    ("TransGame/Localization/INT/Engine.int", "AccessControl.NeedPassword",
     "Engine.AccessControl.NeedPassword"),
    ("TransGame/Localization/INT/Engine.int", "AccessControl.WrongPassword",
     "Engine.AccessControl.WrongPassword"),
    # The Escalation rules line, the lobby's status line, the player list's columns.
    ("TransGame/Localization/INT/TransGame.int", "TnOnlineGameSettingsSV.RulesFormat",
     "TransGame.TnOnlineGameSettingsSV.RulesFormat"),
    ("TransGame/Localization/INT/TransGame.int", "TnGameReplicationInfoLobby.WaitingForHostToStartStatus",
     "TransGame.TnGameReplicationInfoLobby.WaitingForHostToStartStatus"),
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfo.PlayerName.FriendlyName",
     "TransGame.TnPlayerReplicationInfo.PlayerName.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfo.Score.FriendlyName",
     "TransGame.TnPlayerReplicationInfo.Score.FriendlyName"),
    # The starting scoreboard's other columns (the player list's 'InGame' view).
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfo.Kills.FriendlyName",
     "TransGame.TnPlayerReplicationInfo.Kills.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfo.Deaths.FriendlyName",
     "TransGame.TnPlayerReplicationInfo.Deaths.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfoSurvival._Ability0.FriendlyName",
     "TransGame.TnPlayerReplicationInfoSurvival._Ability0.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfoSurvival._Revives.FriendlyName",
     "TransGame.TnPlayerReplicationInfoSurvival._Revives.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int",
     "TnPlayerReplicationInfoSurvival._SurvivalMoney.FriendlyName",
     "TransGame.TnPlayerReplicationInfoSurvival._SurvivalMoney.FriendlyName"),
    # The scoreboard's in a match: each player's class and his level in it, as the
    # lobby's view of the list has them.
    ("TransGame/Localization/INT/TransGame.int", "TnPlayerReplicationInfo.PlayerLevel.FriendlyName",
     "TransGame.TnPlayerReplicationInfo.PlayerLevel.FriendlyName"),
    # The maps' names (their map infos): Downfall, Oblivion, Ancients.
    ("TransGame/Localization/INT/TransGame.int", "ESC_Iacon_BASE_m TnDataProvider_MapInfo.FriendlyName",
     "TransGame.ESC_Iacon_BASE_m.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "ESC_Ark_BASE_m TnDataProvider_MapInfo.FriendlyName",
     "TransGame.ESC_Ark_BASE_m.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "ESC_Temple_BASE_m TnDataProvider_MapInfo.FriendlyName",
     "TransGame.ESC_Temple_BASE_m.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "ESC_Gladiator_BASE_m TnDataProvider_MapInfo.FriendlyName",
     "TransGame.ESC_Gladiator_BASE_m.FriendlyName"),
    ("TransGame/Localization/INT/TransContent.int", "TnTeamInfoAutobots.TeamName",
     "TransContent.TnTeamInfoAutobots.TeamName"),
    # (The Decepticon maps' lobby lists the players under the Decepticons.)
    ("TransGame/Localization/INT/TransContent.int", "TnTeamInfoDecepticons.TeamName",
     "TransContent.TnTeamInfoDecepticons.TeamName"),
    # The keyboard layouts' names (TransInput.ini's keyboard control schemes).
    ("TransGame/Localization/INT/TransGame.int", "Kbd80ControlScheme TnDataProvider_ControlScheme.FriendlyName",
     "TransGame.Kbd80ControlScheme.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "Kbd81ControlScheme TnDataProvider_ControlScheme.FriendlyName",
     "TransGame.Kbd81ControlScheme.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "Kbd82ControlScheme TnDataProvider_ControlScheme.FriendlyName",
     "TransGame.Kbd82ControlScheme.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "Kbd83ControlScheme TnDataProvider_ControlScheme.FriendlyName",
     "TransGame.Kbd83ControlScheme.FriendlyName"),
    ("TransGame/Localization/INT/TransGame.int", "KbdLeftyControlScheme TnDataProvider_ControlScheme.FriendlyName",
     "TransGame.KbdLeftyControlScheme.FriendlyName"),
] + [
    # The controller's layouts' names (TransInput.ini's control schemes for the
    # controller: the Controller Layout screen's rows' values).
    ("TransGame/Localization/INT/TransGame.int", "%s TnDataProvider_ControlScheme.FriendlyName" % name,
     "TransGame.%s.FriendlyName" % name)
    for name in ("RobotAltSideSwitchControlScheme", "RobotAltTransformControlScheme",
                 "RobotAltSprintControlScheme", "RobotAltBumpersAndTriggersControlScheme",
                 "RobotDPadReload", "VehicleInvertAscensionControlScheme",
                 "GrimlockControlScheme", "GrimlockAltControlScheme", "SwapSticksControlScheme")
]

FONTS = {"RegisterDemiBold": "$NormalFont", "RegisterMedium": "$TitleFont", "RegisterLight": "$HeaderFont",
         # (The Customize lists' prices: price_txt.)
         "Energon Numbers Wide": "$NumberFontWide"}

# Coalesced.int: a count, then each packed file as two strings (its name and its
# text), each an int32 length (negative for UTF-16) and the characters. The
# characters are XORed with this 61-byte key, which runs on from one string to the
# next (starting at key byte 8); each string's last character, its terminating
# zero, is left as it is and doesn't use up a key byte.
COALESCED_KEY = b"cfszdcvnas;dwepo2345098]qw]{}p2039458pseasdfzcvvp;aseiurwefsd"


# --- text ----------------------------------------------------------------------

def coalesced(path):
    """The files packed in a Coalesced.int, as {path: text} (paths like
    TransGame/Localization/INT/UIText.int)."""
    d = open(path, "rb").read()
    key = COALESCED_KEY
    strings = []
    at = 8
    pos = 4
    while pos < len(d) - 4:
        n = struct.unpack_from("<i", d, pos)[0]
        wide = n < 0
        count = -n if wide else n
        start = pos + 4
        if wide:
            # UTF-16: one key byte per character, on its low byte.
            chars = [(d[start + 2 * j] ^ key[(at + j) % len(key)]) | (d[start + 2 * j + 1] << 8)
                     for j in range(max(count - 1, 0))]
            strings.append("".join(map(chr, chars)))
            pos = start + 2 * count
        else:
            strings.append(bytes(d[start + j] ^ key[(at + j) % len(key)]
                                 for j in range(max(count - 1, 0))).decode("latin-1"))
            pos = start + count
        at += max(count - 1, 0)
    return {strings[i].replace("\\", "/").replace("%ROOTDIR%", ""): strings[i + 1]
            for i in range(0, len(strings) - 1, 2)}


def ini(text):
    """{Section.Key: value}; the first value wins when a key repeats."""
    out = {}
    section = ""
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
        elif "=" in line and not line.startswith(";"):
            k, v = line.split("=", 1)
            out.setdefault("%s.%s" % (section, k.strip()), v.strip())
    return out


# --- fonts ---------------------------------------------------------------------

def glyph_contours(body, pos):
    """A DefineFont3 glyph shape as contours of ('L', x, y) / ('Q', cx, cy, x, y)
    steps from a start point, in twips (20 per pixel of a 1024 px em)."""
    b = gfx_io.Bits(body, pos)
    nf = b.ub(4)
    nl = b.ub(4)
    x = y = 0
    contours = []
    cur = None
    while True:
        if b.ub(1) == 0:
            flags = b.ub(5)
            if flags == 0:
                break
            if flags & 1:
                n = b.ub(5)
                x = b.sb(n)
                y = b.sb(n)
                cur = [(x, y)]
                contours.append(cur)
            if flags & 2:
                b.ub(nf)
            if flags & 4:
                b.ub(nf)
            if flags & 8:
                b.ub(nl)
            if flags & 16:
                raise ValueError("new styles in a glyph")
        else:
            if cur is None:
                cur = [(x, y)]
                contours.append(cur)
            if b.ub(1):
                n = b.ub(4) + 2
                if b.ub(1):
                    dx = b.sb(n)
                    dy = b.sb(n)
                elif b.ub(1):
                    dx, dy = 0, b.sb(n)
                else:
                    dx, dy = b.sb(n), 0
                x += dx
                y += dy
                cur.append(("L", x, y))
            else:
                n = b.ub(4) + 2
                cx = x + b.sb(n)
                cy = y + b.sb(n)
                x = cx + b.sb(n)
                y = cy + b.sb(n)
                cur.append(("Q", cx, cy, x, y))
    return [c for c in contours if len(c) > 2]


def parse_font(body):
    b = gfx_io.Bits(body)
    b.u16()
    flags = b.u8()
    b.u8()
    name = b.bytes(b.u8()).rstrip(b"\0").decode("latin-1")
    n = b.u16()
    wide = bool(flags & 0x08)
    base = b.byte
    offs = [struct.unpack_from("<I" if wide else "<H", body, base + i * (4 if wide else 2))[0]
            for i in range(n + 1)]
    code_off = offs[n]
    glyphs = [glyph_contours(body, base + offs[i]) for i in range(n)]
    q = base + code_off
    codes = [struct.unpack_from("<H", body, q + 2 * i)[0] for i in range(n)]
    q += 2 * n
    asc, desc, lead = struct.unpack_from("<HHh", body, q)
    q += 6
    adv = struct.unpack_from("<%dh" % n, body, q)
    return {"name": name, "glyphs": glyphs, "codes": codes, "advance": adv,
            "ascent": asc, "descent": desc}


def build_ttf(f, path):
    """Writes a DefineFont3 font as TrueType (1024 units per em, as the movie)."""
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.reverseContourPen import ReverseContourPen
    from fontTools.pens.ttGlyphPen import TTGlyphPen

    def flat(c):
        pts = [c[0]]
        for s in c[1:]:
            if s[0] == "L":
                pts.append((s[1], s[2]))
            else:
                px, py = pts[-1]
                for t in (0.25, 0.5, 0.75, 1.0):
                    pts.append(((1 - t) ** 2 * px + 2 * (1 - t) * t * s[1] + t * t * s[3],
                                (1 - t) ** 2 * py + 2 * (1 - t) * t * s[2] + t * t * s[4]))
        return pts

    def inside(pt, poly):
        x, y = pt
        c = False
        for i in range(len(poly) - 1):
            (x0, y0), (x1, y1) = poly[i], poly[i + 1]
            if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
                c = not c
        return c

    def area(p):
        return sum(p[i][0] * p[i + 1][1] - p[i + 1][0] * p[i][1] for i in range(len(p) - 1)) / 2

    k = 1 / 20  # twips -> font units
    order = [".notdef"]
    cmap = {}
    glyf = {".notdef": TTGlyphPen(None).glyph()}
    hmtx = {".notdef": (512, 0)}
    for i, (code, cons) in enumerate(zip(f["codes"], f["glyphs"])):
        gname = "uni%04X" % code if code else "g%d" % i
        if gname in glyf:
            continue
        order.append(gname)
        cmap[code] = gname
        polys = [flat(c) for c in cons]
        pen = TTGlyphPen(None)
        for j, c in enumerate(cons):
            # Nesting decides outer and inner contours; TrueType wants outer
            # contours clockwise with y up (the movie's y points down).
            depth = sum(1 for m, pm in enumerate(polys) if m != j and inside(polys[j][0], pm))
            outer = depth % 2 == 0
            clockwise = area([(x, -y) for x, y in polys[j]]) < 0
            target = ReverseContourPen(pen) if clockwise != outer else pen
            x0, y0 = c[0]
            target.moveTo((round(x0 * k), round(-y0 * k)))
            for s in c[1:]:
                if s[0] == "L":
                    target.lineTo((round(s[1] * k), round(-s[2] * k)))
                else:
                    target.qCurveTo((round(s[1] * k), round(-s[2] * k)),
                                    (round(s[3] * k), round(-s[4] * k)))
            target.closePath()
        glyf[gname] = pen.glyph()
        hmtx[gname] = (max(0, round(f["advance"][i] * k)), 0)
    fb = FontBuilder(1024, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyf)
    for g in order:
        gl = glyf[g]
        if hasattr(gl, "recalcBounds"):
            gl.recalcBounds(fb.font["glyf"])
    fb.setupHorizontalMetrics({g: (hmtx[g][0], getattr(glyf[g], "xMin", 0) or 0) for g in order})
    asc = round(f["ascent"] * k)
    desc = round(f["descent"] * k)
    fb.setupHorizontalHeader(ascent=asc, descent=-desc)
    # Font loaders (fontdb, which Bevy uses) need the PostScript name as well as
    # the family name.
    fb.setupNameTable({"familyName": f["name"], "styleName": "Regular",
                       "fullName": f["name"], "psName": f["name"],
                       "uniqueFontIdentifier": f["name"], "version": "Version 1.000"})
    fb.setupOS2(sTypoAscender=asc, sTypoDescender=-desc, sTypoLineGap=0,
                usWinAscent=asc, usWinDescent=desc)
    fb.setupPost()
    fb.save(path)


# --- vector art -------------------------------------------------------------------

def load_movie(package, name, tmp):
    """A Scaleform movie cooked into a package, parsed."""
    open(tmp, "wb").write(swf_movie(package, name))
    try:
        return gfx_io.Movie(tmp)
    finally:
        os.remove(tmp)


def shape_loops(movie, cid, m=gfx_io.IDENT):
    """A shape's (or a sprite's first frame's) filled outlines as closed loops of
    points, in the character's coordinates."""
    c = movie.chars[cid]
    out = []
    if c["kind"] == "shape":
        for _, f0, f1, _, segs in c["paths"]:
            if f0 or f1:
                out += [[gfx_render._apply(m, x, y) for x, y in loop]
                        for loop in gfx_render._loops(segs)]
    elif c["kind"] == "sprite":
        for _, pl in sorted(c["frames"][0].items()):
            if "clip" not in pl and "id" in pl:
                out += shape_loops(movie, pl["id"], gfx_io.mat_mul(m, pl.get("matrix", gfx_io.IDENT)))
    return out


def rasterize(movie, cid, scale, path):
    """Draws a vector character in white (its shape as the alpha; the demo tints
    it), even-odd filled and supersampled. The image covers the character's bounds
    plus one movie pixel on every side."""
    from PIL import Image, ImageChops, ImageDraw
    loops = [l for l in shape_loops(movie, cid) if len(l) > 2]
    xs = [x for l in loops for x, _ in l]
    ys = [y for l in loops for _, y in l]
    x0, y0 = min(xs) - 1, min(ys) - 1
    w = round((max(xs) + 1 - x0) * scale)
    h = round((max(ys) + 1 - y0) * scale)
    sx = w / (max(xs) + 1 - x0)
    sy = h / (max(ys) + 1 - y0)
    ss = 4
    acc = Image.new("1", (w * ss, h * ss), 0)
    for l in loops:
        mask = Image.new("1", acc.size, 0)
        ImageDraw.Draw(mask).polygon([((x - x0) * sx * ss, (y - y0) * sy * ss) for x, y in l], fill=1)
        acc = ImageChops.logical_xor(acc, mask)
    im = Image.new("RGBA", (w, h), (255, 255, 255, 0))
    im.putalpha(acc.convert("L").resize((w, h), Image.LANCZOS))
    im.save(path)


# --- the controller map ----------------------------------------------------------

# Settings > Controls > Controller Layout: the controller's picture and its labels,
# as SettingsMenu_GFX's mc_controllerMapXbox360 lays them out (attached at 0, 0 by
# the screen's last item, mc_controllerMap). Each label (an hms.controls.
# ControllerLabel) is its control's description, the game's text, beside its
# lines and dot or its button's icon (Gamepad_L1 and the others, imported from
# ButtonIcons.swf: textures in the same export); those are drawn here, PADMAP_SCALE
# times the movie's size, with a white copy for the white the label comes in as.
# padmap.txt, in the movie's pixels from the map's origin:
#
#   picture <file> <x> <y> <width> <height>
#   label <key> <file> <x> <y> <width> <height> <left|right> <x0> <x1> <y0> <size> <colour>
#   none <x0> <x1> <y0> <size> <colour>
#
# a label's text in a field from x0 to x1 with its top at y0 (the field's own: its
# text 2 pixels in), aligned to its left or right, $NormalFont at <size>, in
# <colour> (hex); `none` is mc_controllerMapNone's line, centred, for no controller.
PADMAP_SCALE = 3
PADMAP_LABELS = [
    ("buttonL1_mc", "XboxTypeS_LeftShoulder", "lb"),
    ("buttonL2_mc", "XboxTypeS_LeftTrigger", "lt"),
    ("buttonBack_mc", "XboxTypeS_Back", "back"),
    ("leftStick_mc", "XboxTypeS_LeftX", "left_stick"),
    ("buttonL3_mc", "XboxTypeS_LeftThumbstick", "ls"),
    ("dpadUp_mc", "XboxTypeS_DPad_Up", "dpad_up"),
    ("dpadLeft_mc", "XboxTypeS_DPad_Left", "dpad_left"),
    ("dpadRight_mc", "XboxTypeS_DPad_Right", "dpad_right"),
    ("dpadDown_mc", "XboxTypeS_DPad_Down", "dpad_down"),
    ("buttonR1_mc", "XboxTypeS_RightShoulder", "rb"),
    ("buttonR2_mc", "XboxTypeS_RightTrigger", "rt"),
    ("buttonStart_mc", "XboxTypeS_Start", "start"),
    ("buttonY_mc", "XboxTypeS_Y", "y"),
    ("buttonB_mc", "XboxTypeS_B", "b"),
    ("buttonX_mc", "XboxTypeS_X", "x"),
    ("buttonA_mc", "XboxTypeS_A", "a"),
    ("rightStick_mc", "XboxTypeS_RightX", "right_stick"),
    ("buttonR3_mc", "XboxTypeS_RightThumbstick", "rs"),
]


class MapArt(gfx_render.Renderer):
    """The map's art without its text, and with its lines' gradients (a linear
    gradient's ramp drawn as a bitmap over the gradient square)."""

    def _text(self, canvas, c, m, cx):
        pass

    def _shape(self, canvas, c, m, cx):
        from PIL import Image
        plain = dict(c, paths=[])
        for set_i, f0, f1, ln, segs in c["paths"]:
            fills, _ = c["styles"][set_i]
            if any(fi and "stops" in fills[fi - 1] for fi in (f0, f1)):
                for fi in (f0, f1):
                    if not fi or "stops" not in fills[fi - 1]:
                        continue
                    f = fills[fi - 1]
                    edges = [[gfx_render._apply(m, x, y) for x, y in gfx_render._flatten(seg)]
                             for seg in segs]
                    mask = gfx_render.fill_mask(canvas.size, edges, self.scale)
                    ramp = Image.new("RGBA", (256, 1))
                    stops = f["stops"]
                    for i in range(256):
                        lo = max([s for s in stops if s[0] <= i], key=lambda s: s[0], default=stops[0])
                        hi = min([s for s in stops if s[0] >= i], key=lambda s: s[0], default=stops[-1])
                        t = 0.0 if hi[0] == lo[0] else (i - lo[0]) / (hi[0] - lo[0])
                        ramp.putpixel((i, 0), tuple(round(lo[1][k] + (hi[1][k] - lo[1][k]) * t)
                                                    for k in range(4)))
                    # (Padded at its ends, as a gradient's ends carry on.)
                    pad = 256
                    wide = Image.new("RGBA", (256 + 2 * pad, 64))
                    for x in range(wide.width):
                        col = ramp.getpixel((min(max(x - pad, 0), 255), 0))
                        for y in range(64):
                            wide.putpixel((x, y), col)
                    g = f["matrix"]
                    # The ramp's pixel x spans the gradient square, -16384 to 16384
                    # twips (its 64 rows the same, tall).
                    k = 32768.0 / 256 / 20
                    gm = (g[0] * k, g[1] * k, g[2] * 512.0 / 20, g[3] * 512.0 / 20, g[4], g[5])
                    gm = gfx_io.mat_mul(gm, (1, 0, 0, 1, -128 - pad, -32))
                    self._blit(canvas, wide, gfx_io.mat_mul(m, gm), cx, mask)
            else:
                plain["paths"].append((set_i, f0, f1, ln, segs))
        if plain["paths"]:
            super()._shape(canvas, plain, m, cx)


def char_bounds(movie, cid, m):
    """A character's bounds (its shapes', not its texts'), under `m`."""
    c = movie.chars.get(cid)
    if not c:
        return None
    if c["kind"] == "shape":
        x0, x1, y0, y1 = c["bounds"]
        pts = [gfx_render._apply(m, x, y) for x in (x0, x1) for y in (y0, y1)]
        return (min(p[0] for p in pts), min(p[1] for p in pts),
                max(p[0] for p in pts), max(p[1] for p in pts))
    if c["kind"] != "sprite":
        return None
    out = None
    for _, pl in sorted(c["frames"][0].items()):
        if "id" not in pl or "clip" in pl:
            continue
        b = char_bounds(movie, pl["id"], gfx_io.mat_mul(m, pl.get("matrix", gfx_io.IDENT)))
        if b:
            out = b if out is None else (min(out[0], b[0]), min(out[1], b[1]),
                                          max(out[2], b[2]), max(out[3], b[3]))
    return out


def text_field(movie, cid, m):
    """The first text field under a character: (its bounds under `m`, its text
    character)."""
    c = movie.chars.get(cid)
    if not c:
        return None
    if c["kind"] == "text":
        x0, x1, y0, y1 = c["bounds"]
        a = gfx_render._apply(m, x0, y0)
        b = gfx_render._apply(m, x1, y1)
        return (a[0], b[0], a[1]), c
    if c["kind"] == "sprite":
        for _, pl in sorted(c["frames"][0].items()):
            if "id" in pl:
                found = text_field(movie, pl["id"], gfx_io.mat_mul(m, pl.get("matrix", gfx_io.IDENT)))
                if found:
                    return found
    return None


def write_controller_map(cooked, tex_dir, out):
    """img/pad_*.png and padmap.txt (see PADMAP_LABELS)."""
    import re
    from PIL import Image
    movie = load_movie(os.path.join(cooked, "UI_FrontEnd_m.xxx"), "UI_GFxSettings_p.SettingsMenu_GFX",
                       os.path.join(out, "settings.gfx"))
    # (The button icons it imports, as the export names them.)
    from build_hud import imported_fonts
    for cid, name in imported_fonts(movie.tags, "../ButtonIcons/ButtonIcons.swf").items():
        movie.images[cid] = {"id": cid, "name": name + ".tga", "w": 0, "h": 0}
    art = MapArt(movie, tex_dir, scale=PADMAP_SCALE)
    xbox = movie.chars[movie.exports["mc_controllerMapXbox360"]]
    by_name = {pl.get("name"): pl for pl in xbox["frames"][0].values()}
    lines = ["# The Controller Layout screen's controller and labels, made by tools/build_menu.py"]
    # The picture: the first thing placed, unnamed (its image the shape's fill).
    pic = next(pl for _, pl in sorted(xbox["frames"][0].items()) if not pl.get("name"))
    pm = pic.get("matrix", gfx_io.IDENT)
    shape = movie.chars[movie.chars[pic["id"]]["frames"][0][1]["id"]]
    image = next(f["bitmap"] for f in shape["styles"][0][0] if f.get("bitmap") in movie.images)
    src = os.path.join(tex_dir, os.path.splitext(movie.images[image]["name"])[0] + ".png")
    shutil.copyfile(src, os.path.join(out, "img", "pad_controller.png"))
    x0, x1, y0, y1 = shape["bounds"]
    lines.append("picture pad_controller.png %g %g %g %g" % (pm[4] + x0, pm[5] + y0, x1 - x0, y1 - y0))
    for name, key, stem in PADMAP_LABELS:
        pl = by_name[name]
        m = pl.get("matrix", gfx_io.IDENT)
        b = char_bounds(movie, pl["id"], m)
        bx0, by0 = math.floor(b[0]) - 1, math.floor(b[1]) - 1
        bx1, by1 = math.ceil(b[2]) + 1, math.ceil(b[3]) + 1
        w, h = bx1 - bx0, by1 - by0
        canvas = Image.new("RGBA", (w * PADMAP_SCALE, h * PADMAP_SCALE), (0, 0, 0, 0))
        art.draw(canvas, pl["id"], gfx_io.mat_mul((1, 0, 0, 1, -bx0, -by0), m),
                 ([1.0, 1.0, 1.0, 1.0], [0, 0, 0, 0]))
        canvas.save(os.path.join(out, "img", "pad_%s.png" % stem))
        white = Image.new("RGBA", canvas.size, (255, 255, 255, 0))
        white.putalpha(canvas.getchannel("A"))
        white.save(os.path.join(out, "img", "pad_%s_white.png" % stem))
        (fx0, fx1, fy0), t = text_field(movie, pl["id"], m)
        align = "right" if 'align="right"' in (t.get("text") or "") else "left"
        size = re.search(r'size="(\d+)"', t["text"]).group(1)
        colour = re.search(r'color="#([0-9a-fA-F]{6})"', t["text"]).group(1)
        lines.append("label %s pad_%s.png %d %d %d %d %s %g %g %g %s %s"
                     % (key, stem, bx0, by0, w, h, align, fx0, fx1, fy0, size, colour))
    none = movie.chars[movie.exports["mc_controllerMapNone"]]
    first = next(pl for pl in none["frames"][0].values() if pl.get("name") == "controllerNoneLabel1_mc")
    (fx0, fx1, fy0), t = text_field(movie, first["id"], first.get("matrix", gfx_io.IDENT))
    size = re.search(r'size="(\d+)"', t["text"]).group(1)
    colour = re.search(r'color="#([0-9a-fA-F]{6})"', t["text"]).group(1)
    lines.append("none %g %g %g %s %s" % (fx0, fx1, fy0, size, colour))
    with open(os.path.join(out, "padmap.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")


# --- movie -----------------------------------------------------------------------

def encode_movie(src, dst):
    """BG_RustSea.bik (1280x720, 29.97 fps, 60 s; it starts and ends on white so it
    loops seamlessly) as H.264 Baseline, the profile the demo's decoder handles."""
    r = subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", src, "-an",
                        "-c:v", "libx264", "-profile:v", "baseline", "-preset", "slow",
                        "-crf", "23", "-pix_fmt", "yuv420p", "-g", "300",
                        "-f", "h264", dst], capture_output=True)
    if r.returncode != 0:
        print("  ! ffmpeg failed:", r.stderr.decode()[:300])
        return False
    return True


def encode_credits(src, video_dst, audio_dst):
    """TFOC_Ending_Credits.bik (1280x720, 29.97 fps, 2:40) as H.264 Baseline, like
    the background but packed tighter (the slowest preset, CRF 26: about 28 MB,
    the credits' text as sharp as the Bink's), and its sound as Ogg Vorbis. The
    Bink holds six mono tracks; only the first two (left and right) have sound."""
    r = subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", src, "-an",
                        "-c:v", "libx264", "-profile:v", "baseline", "-preset", "veryslow",
                        "-crf", "26", "-pix_fmt", "yuv420p", "-g", "600",
                        "-f", "h264", video_dst], capture_output=True)
    if r.returncode != 0:
        print("  ! ffmpeg failed:", r.stderr.decode()[:300])
        return False
    r = subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", src,
                        "-filter_complex", "[0:1][0:2]join=inputs=2:channel_layout=stereo[a]",
                        "-map", "[a]", "-c:a", "libvorbis", "-q:a", "6", audio_dst],
                       capture_output=True)
    if r.returncode != 0:
        print("  ! ffmpeg failed:", r.stderr.decode()[:300])
        return False
    return True


def write_text(transgame, out):
    """The fonts (fonts/*.ttf) and menu.txt: the fonts' advance widths and the
    menu's text."""
    os.makedirs(os.path.join(out, "fonts"), exist_ok=True)
    print("reading the fonts from Startup_int.xxx")
    raw = swf_movie(os.path.join(transgame, "CookedPC", "Startup_int.xxx"), "UI_GFxFonts_p.Fonts_EFIGS_GFX")
    tmp = os.path.join(out, "Fonts.gfx")
    open(tmp, "wb").write(raw)
    _, _, tags = gfx_io.load(tmp)
    os.remove(tmp)
    lines = ["# Fall of Cybertron's main menu text and font widths, made by tools/build_menu.py"]
    for code, body in tags:
        if code != 75:
            continue
        f = parse_font(body)
        if f["name"] not in FONTS:
            continue
        # (Named without spaces: menu.txt's lines are split at them.)
        name = f["name"].replace(" ", "")
        build_ttf(dict(f, name=name), os.path.join(out, "fonts", name + ".ttf"))
        for c, a in sorted(zip(f["codes"], f["advance"])):
            if 32 <= c < 127:
                lines.append("advance %s %d %.6f" % (name, c, (a & 0xFFFF) / 20480.0))

    print("reading the text from Coalesced.int")
    files = coalesced(os.path.join(transgame, "Localization", "Cooked", "PC", "Coalesced.int"))
    text = ini(files["TransGame/Localization/INT/UIText.int"])
    for key in STRINGS:
        if key in text:
            lines.append("string %s %s" % (key, text[key]))
        else:
            print("  ! no text for", key)
    other = {}
    for file, key, name in OTHER_STRINGS:
        if file not in other:
            other[file] = ini(files.get(file, ""))
        value = other[file].get(key)
        if value is not None:
            lines.append("string %s %s" % (name, value))
        else:
            print("  ! no text for", key, "in", file)

    with open(os.path.join(out, "menu.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")


def main(transgame, tex_dir, assets, pause_tex_dir=None, lobby_tex_dir=None):
    cooked = os.path.join(transgame, "CookedPC")
    out = os.path.join(assets, "menu")
    os.makedirs(os.path.join(out, "img"), exist_ok=True)
    os.makedirs(os.path.join(out, "fonts"), exist_ok=True)

    print("copying images")
    for name, src, _ in IMAGES:
        path = os.path.join(tex_dir, src + ".png")
        if os.path.exists(path):
            shutil.copyfile(path, os.path.join(out, "img", name))
        else:
            print("  ! %s not in the umodel export" % src)
    for name, src, _ in PAUSE_IMAGES:
        path = os.path.join(pause_tex_dir or tex_dir, src + ".png")
        if os.path.exists(path):
            shutil.copyfile(path, os.path.join(out, "img", name))
        else:
            print("  ! %s not found (export it from mp_shared.xxx)" % src)
    for name, src, _ in LOBBY_IMAGES:
        path = os.path.join(lobby_tex_dir or tex_dir, src + ".png")
        if os.path.exists(path):
            shutil.copyfile(path, os.path.join(out, "img", name))
        else:
            print("  ! %s not found (export UI_Lobby_ESC_m.xxx)" % src)

    print("drawing the lobby's icons")
    movies = {}
    for name, package, movie, char, scale in VECTOR_IMAGES:
        if (package, movie) not in movies:
            movies[package, movie] = load_movie(os.path.join(cooked, package), movie,
                                                os.path.join(out, "movie.gfx"))
        m = movies[package, movie]
        rasterize(m, m.exports[char] if isinstance(char, str) else char, scale,
                  os.path.join(out, "img", name))

    print("drawing the controller map")
    try:
        write_controller_map(cooked, tex_dir, out)
    except (OSError, KeyError, StopIteration) as e:
        # (The rest of the menus build without it: the Controller Layout screen
        # then shows no controller.)
        print("  ! the controller map couldn't be drawn: %r" % (e,))

    write_text(transgame, out)

    movie = os.path.join(transgame, "Movies", "BG_RustSea.bik")
    if os.path.exists(movie):
        print("encoding the background movie (takes a few minutes)")
        encode_movie(movie, os.path.join(out, "background.h264"))
    else:
        print("  ! %s not found: the menu shows a still background" % movie)
    credits = os.path.join(transgame, "Movies", "TFOC_Ending_Credits.bik")
    if os.path.exists(credits):
        print("encoding the ending credits (takes several minutes)")
        encode_credits(credits, os.path.join(out, "credits.h264"), os.path.join(out, "credits.ogg"))
    else:
        print("  ! %s not found: Extras > Credits does nothing" % credits)
    print("wrote", out)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--text":
        write_text(sys.argv[2], os.path.join(sys.argv[3], "menu"))
    elif len(sys.argv) not in (4, 5, 6):
        sys.exit(__doc__)
    else:
        main(*sys.argv[1:])
