# foc_esc (Rust + Bevy)

A small third-person shooter built with the Bevy engine (0.18) that uses models
you extract from **your own** copy of Transformers: Fall of Cybertron.

You can run it right away: any model it can't find is swapped for a simple
placeholder made of boxes, so you see the game working before you extract anything.

**What's new in this update** (split screen; building the assets from your game
and checking the build; Downfall's enemies)

- **Split screen: two players on one PC.** In the Escalation lobby, while you host
  a game, a second player presses **Start** on a controller of his own and joins
  as **Player 2** (Start Game's hint says how; **B** on his controller takes
  him out again). The first player keeps the keyboard and mouse, or the
  first controller. As the match starts each chooses his character, Player 2
  after Player 1 (the Choose Character screen comes up again for him, PLAYER 2
  over it), and the window is shared: one view above the other, or side by side
  (**Settings > Display > Split Screen**), a thin black line between them. Each
  player has his own camera, with the game's field of view across his half (no
  squeezed or stretched picture), his whole HUD (health, ammo and weapons,
  energon, prompts, crosshair, messages and awards, the kill feed, the markers),
  his own energon to spend, his own pause menu (Start on his controller) and his
  own voice for his lines. The effects are drawn for each view (sprites and
  trails turned to each camera, the flares in each), each sound is heard as loud
  as the player nearest it would hear it, and the live lights and the characters'
  shadows are those nearest either player. The waves, the team's lives and the
  revives count both players: a downed player can be revived by the other, and
  a dead one spectates him. Player 2's experience isn't kept (the progress file
  is the first player's), and while he is in, the game is closed to players
  online (one who tries is told it is full). See *Split screen* below.
  (`src\split.rs`, new, and most of the game's systems, now for each of this PC's
  players; `src\net.rs`'s guest; the tests
  `two_players_share_the_window_with_a_line_between`,
  `a_split_view_keeps_an_ordinary_screens_shape_for_its_field_of_view`,
  `a_split_screen_guest_takes_a_slot_and_closes_the_game` and
  `each_sound_is_heard_by_the_nearest_camera`.)
- **A camera pointed at another picture works out its size at once.** Bevy works
  a camera's picture size out again when its window or picture changes size, but
  not when the camera is pointed at another picture, so for a frame or more it
  could draw with the old size; where cameras share a picture (split screen's,
  or the Render Scale's as the window changes size) one of another size is a
  graphics error that stops the demo (split screen's tests ran into it). The
  demo now has Bevy work it out again as soon as a camera is pointed elsewhere
  (`src\graphics.rs`'s `refresh_retargeted`).
- **Split screen: Player 2's half no longer shows the first player's effects.**
  Each view draws its part of the window and then works its effects (the map's
  colour grade, the depth of field, the light shafts) over the whole of the
  picture the window's cameras share, as Bevy's cameras sharing a picture do; only
  its own part was meant to reach the screen. But the menus' camera and the Flash
  camera draw over the game across the whole window, and with that they put the
  first player's effects, worked over Player 2's half, on the screen: his half
  came out darker and greener (graded twice), with bright streaks running down it
  from the lights at the foot of the first player's half (his depth of field's
  blur, which stops at the edge of his half, drawn down from there), on the line
  between the halves too. Split screen's two views now draw into a picture of
  their own the window's size (as the Render Scale's below 100%), which only each
  view's part reaches, and a camera puts that on the window under the HUDs and
  menus; every game camera is put into that picture as it comes, one made after
  it too (Player 2's, made from the first player's as he joins). With two players
  that takes some more of the graphics card's memory and one more copy of the
  picture each frame (`src\graphics.rs`'s `apply_render_scale`).
- **The robot is yours to move as he unfolds out of his vehicle.** Changing back
  from a vehicle, the robot stood where the vehicle left him till his unfolding was
  over (the whole of its clip, some 1.2 s), which showed most with the Titans' slow
  tanks: Megatron's unfolding is all but done a quarter of a second before its clip
  ends, and he stood still on the spot for a second or so. In the game the robot is
  in the player's hands from the start of the change (its controller takes the
  input away only for a character whose blueprint asks, `DisableTransformationControl`,
  which no class's does: `TnPlayerPawn.OnBeginTransformation`): he keeps the
  vehicle's speed while it is the faster, then walks, runs or strafes as you ask,
  and his unfolding blends into his moves over its last quarter of a second
  (`TnTransformation`'s `BlendOutTime`, 0.25 s). Folding up into a vehicle is as it
  was. (`src\main.rs`'s `move_jazz` and `animate_robot`.)
- **Swapping weapons as the gun comes back out keeps its parts on the arm.** As he
  stops sprinting, or jumps out of a sprint, the gun comes back out of the forearm,
  the forearm folding away on its copy on the forearm's other bones (the CloudB
  bones). A swap then folded the old gun away on those same bones, under the
  forearm's clip, so its parts came off his arm, floated beside him and came back
  as the swap ended. The old gun, only part of the way out, now goes at once, and
  the new one unfolds out of the forearm as usual. (`src\main.rs`'s
  `animate_robot`.)
- **The cars' and tanks' boost as loud as the game's.** The game sends each of a
  sound's layers to the speakers its designers chose (the layer's `PanFrontLeft`,
  `PanFrontRight`, `PanCenter`, `PanBackLeft` and `PanBackRight`), and a car's or
  a tank's boost exhaust loop to none of them for the player in it: he doesn't hear
  it, the others round him do (its cue's smart pan, `kSmartPan_PreferPlayer`). The
  demo played it to its driver too, the loudest of a boost's sounds by far (the
  Infiltrators' sports and muscle cars' at -3 and 0 dB, against -10 dB for the boost's
  own loop), so their boost came out 8 to 14 dB louder than the game's, the Titans'
  tanks', Snarl's and Slug's 9 dB. `tools\build_sounds.py` now writes each layer's
  speaker levels where they aren't a plain sound's (`sounds.txt`'s new `pan` lines)
  and the demo leaves out, for the player himself, a layer the game sends to no
  speaker (`src\sound.rs`'s `flat_level`). The trucks' boost (their exhausts are
  silent in the game's cues) and the jets' are as they were, and so is every other
  sound. This update's `assets\sounds\sounds.txt` has them (or run `extract
  assets.bat` again).
- **Swindle's wheels roll the right way.** On his wheels (boosting), Swindle's
  truck's tyres turned over sideways, round the truck's length, instead of rolling
  forward. Every other vehicle's tyres hang on its wheel bones (`L_Wheel01_XB` and
  the like, their Z across the vehicle, their Y up), which the demo turns them round
  as the game's `SkelControlWheel`s turn the wheels; Swindle's eight hang in pairs
  on bones of their own under his four wheel bones (`L_` and `R_Robo07_XT`, `08`,
  `10` and `11`, the ones his hover pose turns flat), turned another way. Each
  tyre now turns round its wheel bone's axle, and is steered and sprung along its
  wheel bone's up, wherever it hangs (as the vehicle's model has its bones at
  rest), so Swindle's roll as the others' do, his front pairs steer (they didn't),
  and on the springs his right side's drop out of the arches as the left's do (they
  rose into them). Every other vehicle's wheels turn as they did.
  (`src\character.rs`'s `VehicleWheel` and `wheel_frame`, `src\car.rs`'s
  `car_state`; the test `swindles_wheels_roll_round_his_trucks_axles`.)
- **No Grimlock controls in the settings.** The game's Controller Layout screen
  has a Grimlock row (Default and Rebel) and its Keyboard/Mouse Layout screen a
  Grimlock view of each layout's keys, both for the campaign's Grimlock, whom the
  demo doesn't have (Escalation's Grimlock, the Titan's armor set, plays with the
  Titan's controls). Both are gone: Controller Layout has three rows, **Robot**,
  **Vehicle** and **Thumbsticks**, and View Layout two, **Robot** and **Vehicle**.
  The other layouts and their keys are as they were. Your `settings.txt` loads as
  before: a `controller_grimlock_layout` line in it is no longer read, and any
  `bind_grimlock_...` lines go the next time the controls are saved.
  (`src\padlayout.rs`, `src\controls.rs`, `src\menu.rs`, `src\pad.rs`.)
- **A vehicle's boost no longer hangs in the air after it changes back.** A
  boost's glow and trails are let out into the world as the vehicle drives
  (`Trail_SatHomingRocket_FX`'s big soft glow, behind most of the Destroyer's
  trucks among others, lives a second or two where it came out), and as the boost
  stopped for the change back into the robot, what was out of them lived out its
  life: a pink haze hung where the truck had been, in front of the camera, for a
  second after the robot had stood up. What is left of a vehicle's effects now goes
  as the robot comes out of it (as his pieces show) or as the vehicle is hidden.
  Stopping a boost on the move (letting go, or a crash) leaves its glow to fade as
  before. (`src\car.rs`'s `vehicle_effects`.)
- **The menus' questions have the game's orange selection box.** The buttons of
  every prompt (Exit Game's Continue and Cancel, Quit Game's and Return to Lobby's Yes
  and No, Prime Mode's Confirm and Cancel, Join Game's, Display Name's and the Game
  Password's, a message's Ok) show the one in focus as the game does: an orange box
  with a white bar under it and dark lettering, the others dark with light blue
  lettering. The first has the focus as the prompt opens; Left/Right (A/D, the d-pad),
  Tab or the mouse move it, with the game's sounds, and Enter, Space, A or a click
  presses it. Their wording is as it was. (`src\menu.rs`'s `prompt_buttons`.)
- **EXIT GAME asks first**, as the game's does: "Warning: Do you want to exit the
  game?" with Continue and Cancel (the game's own text).
- **Offline Game in the party lobby**, above Join Game (which stands in the game's
  Find Game's place): a game of your own with no network at all (no port opened,
  so nobody online can join it, and no Game Password to type), straight into its
  lobby: Start Game plays it alone, or with split screen's second player (Start on a
  second controller brings him in, as in a game you host). (`src\net.rs`'s
  `host_offline`.)
- **Customize in the Escalation lobby**, above the Difficulty Setting, as the game's
  lobby has it (for the host and for those who join): it opens the same Customize
  screen as the main menu's CUSTOMIZE, and closing it comes back to the lobby. If the
  host starts the countdown while you are in it (or the game ends), the screen closes
  by itself, your builds kept as far as you had accepted them, and the lobby shows
  the countdown.
- **The version on the main menu.** The main menu shows the demo's version at its
  bottom left (in the window's corner), small and faint, fading in with the menu,
  so players can tell which version they have. A demo installed with
  `make installer.bat`'s Setup shows that Setup's version, the date and time it
  was made (for instance `Version 2026.11.7.1512` from `Cybertron Escalation Setup
  2026.11.7.1512.exe`): the installer builds `foc_esc.exe` with it
  (`FOC_ESC_VERSION`), and File Explorer's Properties > Details shows the same as
  the program's version. A build of your own (`boot demo cargo.bat`) shows the
  package's, `Version 1.0.0 (dev build)`. (`src\menu.rs`'s `version_label`,
  `build.rs`, `tools\make_installer.py`.)
- **The Heal Beam's arm stays up.** While the Scientist heals, his free hand stays
  raised, as in the game: the game's clip (`Skill_Heal`, 1.03 s) brings the hand up
  once and then goes round only its second half, from its loop point
  (`LoopOffsetTime`, 0.54 s). The demo went round the whole clip, so every second
  the hand dropped to his waist and came back up. `tools\build_robot_extras.py` now
  writes the clip with that loop point as it does the Whirlwind's (`LOOPS`), and the
  game plays it once and then round from there (`src\healbeam.rs`'s `clip_time`).
  Only `assets\models\robot_extras.glb`'s heal clip changed; its other 46 clips
  are as they were.
- **The Guardian has both its arm cannons.** In its battle form it showed only the
  cannon on its right arm. The game's Guardian Blaster hangs a cannon on each of its
  two attachment sockets (`WeaponSocket_Primary` and `WeaponSocket_Primary_L`, one
  on each arm), and the demo treated them as a right-hand and a left-hand version of
  one gun, showing the one for the hand it aims with. Both now show while it has
  its gun out (`src\enemies.rs`'s `guns_hung_on_both_arms`, `src\ai\mod.rs`'s
  `read_sockets`); every other enemy still shows its gun in the hand it holds it in.
- **Player 2 joins in the party lobby.** With a controller connected that a second
  player could join on (one other than the first player's), the party lobby shows
  "Press START to join" under your name, in the lobby's orange, pulsing as its
  status does. Start on that controller brings him into the party, listed under
  you as **Player 2** (B on his controller takes him out again), and the game you
  then start with **Offline Game** or **Host a Game** opens its lobby with him
  already in it, ready to play split screen. He stays with you back in the party
  lobby after a game, and leaves the party when you go back to the main menu or
  his controller is unplugged. Joining in the game's lobby itself works as
  before, and puts him in the party too. (`src\menu.rs`'s `party_guest` and
  `lobby_guest`, `src\split.rs`'s `Split::party`; the tests
  `player_2_joins_the_party_and_comes_into_a_hosted_game` and
  `a_game_lobbys_guest_is_in_the_party_too`.)
- **Snipers no longer float off their walls into the sky.** Now and then a Sniper
  clinging to its perch went straight up the wall and on into the sky, still in its
  clinging pose, on every map with Snipers. It happened when a Sniper was crawling
  across its perch (up the wall to the spot it takes off from, say) and its plan
  changed partway, as the alert that sends every enemy at you every five seconds
  can make it. The crawl stopped but left the Sniper pushed along its wall: a
  Sniper has no motion controller to take that off it, and nothing it does on its
  perch after a crawl moves it, so it slid on along the wall's plane, off the
  wall's top, at up to its 75 m/s crawling speed. A crawl now stops the Sniper
  where it is however it ends, as a finished one already did. (`src\ai\sniper.rs`'s
  `Crawl::finish`; the Sniper surveys in `src\ai\mod.rs` now run with the
  alerter, Downfall's shoots its Snipers on their perches too, and each fails if
  a Sniper leaves its perch, flies loose or rises over the map.)
- **No more white blotches on Downfall's upper floors.** The plating round the map
  room glowed in white patches the game doesn't have. The demo couldn't find that
  material's glow: its parent (`IAC_BRDF_Base_MATINST`) is in the packages the game
  loads with every map, `all_shared.xxx` and `mp_shared.xxx`, which the map build
  now reads too, and there its glow colour is black: it doesn't glow. The same goes
  for a few other surfaces whose parents are there: Downfall's train track,
  monitors and Metroplex arm, Ancients' rust signs and an Ignition building piece
  don't glow either, and the ramps' blue strips (Downfall, Oblivion), Downfall's
  pistons and pipes and Ignition's orange train track glow in their materials' own
  colours instead of their colour maps'. Without the false glow the upper floors
  come out darker than before (and darker than the game's, whose plating has a soft
  grey sheen the demo's stand-in material doesn't draw). Only the glow changed: the
  maps' `models\<map>.glb` and four new glow textures (`FloorRamp_MAT_INST_e`,
  `PROP_NEU_Piston_B_MATINST_e`, `PROP_NEU_AssortedGearsAndPipes_MATINST_e`,
  `TrainTrack_orange_MATINST_e`).
  (`tools\ue3\matparams.py`'s `map_params`, read by `tools\build_map.py` and
  `tools\build_motion.py`.)
- **One command builds every asset from the game**: double-click **`extract assets.bat`**
  (or run `python tools\extract_game.py`). It finds the game and UE Viewer, exports
  from the game's packages what each build script reads, runs the scripts in order
  (39 steps: the four maps and their doors, pods, traps and barrels, the waves and
  navigation, Jazz and the guns, the four classes' characters, the enemies and their
  AI and clips, the effects, the HUD, the menus, the garage and Customize, and every
  sound) and logs each one; run it again after a failure and it carries on where it
  stopped. See "Build the assets from your game" below for what to install first.
  With it anyone with the game can build the demo's assets from their own copy.
- **The steps that used to be done by hand are now done by the scripts**: Jazz's
  clips come from the animation sets' `.psa` files (umodel's command line) instead of
  glTF files only umodel's viewer could save; `build_weapon_fx.py --umodel` lists every
  mesh its effects draw and the Diffraction Barrier's meshes are cut by the script;
  `build_enemies.py --umodel` also lists the enemies' texture sets and the shared
  animation sets their anim trees play; `build_fx_textures.py` copies the older
  effects' textures too. `tools\compare_assets.py` compares two assets folders.
- **Every animation clip, whichever UE Viewer you have**: the official UE Viewer builds
  leave out the clips Fall of Cybertron keeps in its own layout (most of its clips:
  some animation sets come out short, the trap sentries' not at all), so the run
  writes each animation set's `.psa` again with `tools\psa_write.py`, straight from
  the game's package, keeping the clips umodel did save as they are. The keys come
  out exactly as a UE Viewer that reads those clips writes them.
- **Fixed on the way**: rebuilding Oblivion and its doors with the current scripts
  would have left its locked doorways' force fields without their material (they
  took it from Ancients' build, which now draws its force fields as effects);
  Oblivion's build now compiles that material into its own effects. And a few lines
  in `tools\ue3\fxmat.py` needed Python 3.12 or newer; they no longer do.
- **Downfall's enemies don't get stuck out of reach**: the Snipers and the jets no
  longer fly into the locked doors' force fields or hang against the map. The
  level's air network has flights straight through several closed doors (14 on
  Downfall) and a few that graze its pillars (the four to the Sniper perch
  `TnSniperPoint_2779` cut through the edge of a big round pillar); a flier sent
  along one ran into it and hung there. A flight through a door is now shut while
  the door is closed, and each flight's room is measured against the map as the
  match starts, so a flier too big for it goes another way. And an enemy no player
  has seen for a minute (30 s once the wave's last few show on the HUD) is taken
  out of play, not killed, and another comes in near the players instead: one
  stranded behind a force field or up a scaffold no longer leaves a wave that can't
  be won. (Downfall's first door also keeps its enemies out: until the start room's
  door is bought, those that can't reach you, such as the Shotgunners on the start
  building's roof, wait outside; buy it and they come down.)
- **A full build compared with the demo's own**: `tools\compare_assets.py` now
  tells the files that hold something different from those that only differ in
  how they were written (sounds encoded again, other line endings, numbers a
  rounding apart, block compression's noise, a model's parts in another order...)
  and says how each differing file differs. Comparing a build from a full copy of
  the game with the demo's own found two things the build got wrong, both fixed:
  effect meshes that have no vertex colours of their own (Ancients' and Ignition's
  force fields, the maps' glow spheres and light beams, Oblivion's space dome,
  space bridge and control tower windows, the garage's glow) came out black or
  speckled, as UE Viewer fills those colours from memory it never sets: the build
  now reads the game's package to know and gives them white, as the game draws
  them (Oblivion's own files had the speckles too); and the traps' flame jets on
  Ignition and Oblivion came out unlit, the doors' build never reading whether the
  game lights them. The text files the build writes now have LF line endings, as
  the demo's own do.
- **Walking into Ancients' rocks, and falling out under the map**: where a rock's
  overhang comes down to the floor (two of them by the Techvolt's room: the big
  rocks west and south-west of it), a player walking in under it was stepped up
  onto its underside, which the movement took for a floor (it is as flat as one,
  and which way a surface faces was never asked), and so into the rock: inside it
  he walked on along the canyon floor, which runs on under the rocks, to where it
  ends, and fell out under the map, falling for seconds until the demo's own limit
  60 m down put him back at the start. A surface facing down is now never stood on
  from below, and stops a player walking in under it where it comes down lower
  than 2.2 m over his feet (or his top less a step: 4.3 m for a Titan, keeping his
  head and the camera out of the rock; a car's own top), as a wall does: above a
  step's height, so the undercut foot of a lump on the floor is still stepped onto
  and a wall's still slid along (`src\collision.rs`; the enemies move as before).
  Which way a surface faces is told by the order of its corners, and the maps'
  collision had that backwards for the meshes the levels place mirrored (134 of
  Ancients' 1,100 rocks and props; a mirror turns a mesh inside out):
  `tools\build_map.py` now turns their triangles round, and `tools\col_winding.py`
  has done the same to the four maps' built collision files
  (`assets\levels\<map>_col.bin`). Of 2,160 players wandering at random round the
  Techvolt's room for about 40 s each, 236 used to end up under the map (106
  Titans); now 18 do (19 Titans), all of them from walking off the far end of the
  giant pipe that lies high across the west of the canyon. And should a player
  still fall out, the game takes out whatever falls below the map's KillZ (its
  TnWorldInfo's: 10 m under Ancients' lowest floor, 20 m under Downfall's and
  Ignition's), so the maps' points files now carry it (`killz`, written by
  `tools\build_map.py`) and a player who falls below it is put straight back
  where he stood a moment before he went over (the game kills him there; the demo
  is kinder).
- **Enemies that stood by you without firing**: a soldier near enough to swing at
  you (its nearest range, within 3 m of you cylinder to cylinder: a Titan circling
  it 5 or 6 m away is that near) could freeze where it stood, its rifle raised and
  turning to face you, neither striking nor firing for as long as you stayed by it
  (the soldier by the health station in your Ignition video). When it gave up
  a swing part way through, its plan dropped for a better one as you came nearer,
  that swing's animation was stopped, and the animation's callback, which here
  comes the next frame (the game's comes the moment it stops), ended the swing the
  soldier had begun in its place; that swing's callback then ended the next, and
  so on: it restarted its swing every other frame, its blow never coming. Each
  swing's callback now ends only its own swing (`src\ai\melee.rs`), so the soldier
  swings at you and its blows land, as in the game; the Scrappers' squad melee,
  which swings the same way, can't freeze like it either. In a headless run of
  Ignition's waves with a player standing by that health station, one soldier had
  stood there for over two minutes; now none does. The test
  `an_attack_given_up_mid_swing_leaves_the_next_alone` checks it. The same runs
  found a second way an enemy froze: one struck just as it stopped walking took the
  hit reaction for one on the move (its stumble, `TR_AI_Navigation_HitReactionPicker`),
  which has none for one standing still, so it asked its animator for no animation
  at all. The game's animator answers such a request by calling back that it
  failed (`TnCustomAnimationPlayer.PlayAnimation`'s `FailedNotify`), and the
  reaction ends; the demo's called back nobody, and the enemy stood waiting for the
  stumble for good (over a minute in those runs, 12 m from the player). It now
  calls back as the game's does (`src\ai\anim.rs`; the test
  `playing_no_animation_fails_it`), which ends any operator left waiting on an
  animation that isn't there.
- **Jets that hung in the air without firing**: a jet hovers to its attack point
  to fire on you (`TnHoverToFaceOperator`), and the hover is done only once its body
  is over the point (the hover's range in its plan is 0.2 uu) and it faces you; but
  the plane's motion controller stops steering as soon as it counts the jet as there
  (`ReachedDestination`: within its radius, 4.45 m, of the point across the ground,
  or past it, and within 8 m of its height). A jet that began its hover already that
  near (come in steeply, or fast enough to coast on to the stop its flight set, half
  a second of its speed further on) was left where it was, 2.3 to 8 m over or under
  the point or some 10 m past it: it waited out the hover's 5 s without firing and
  began it again, until something else came up for it (its pulse attack, when no
  other jet had it and its way down to you was open). The game's script reads the
  same; how often a jet starts its hover that near depends on how it flies in, and
  here it was often: in a headless run of Ignition's jets one hung like that for
  45 s, and others for 6 to 10 s at a time, several times a wave. The hover now takes
  the jet on to the point itself when it is that near and not on it
  (`src\ai\jet.rs`), so it settles there, turns to you and fires. A jet hovering right
  over a player standing still (the point it makes for after hearing him is over
  where he stood) had no way across the ground to face him and waited out its hover
  again and again; any way it faces now counts. The tests
  `a_jet_comes_onto_an_attack_point_it_begins_its_hover_beside` and
  `a_jet_right_over_its_enemy_faces_him_whichever_way_it_is_turned` check them.
- **The cars' and trucks' jumps sound the right way round.** The demo had the two
  jump sounds swapped. A hover jump now plays the hover jump sound
  (`Auto_Jump_Start`, the `*_DRIVE_JUMP_START` cues), and a jump while boosting on
  the wheels plays the boost jump sound (`Auto_Boost_Jump`, `*_BOOST_JUMP_START`), as
  the game's `TnCarForm` plays them (`PlayAscendSound` hovering, `PlayBoostJumpSound`
  boosting). The tanks jump with their hover jump, as before (`src\car.rs`; the
  test `a_hover_jump_and_a_boost_jump_sound_as_the_game_has_them`).
- **The Insecticons buzz as they hover.** Sharpshot's and Kickback's hover sound
  (`VEH_INSECTICON_FLYING_LP`) is the only vehicle engine sound in the game that
  loops back over one-off layers of its own: every 0.31 s it plays its wings' buzz
  (`VEH_INSECT_FLY_BZZ_*`, each at a random pitch and loudness), a rustle
  (`VEH_INSECT_FOLEY_*`) and a small foley (`FS_BOT_SMALL_FOLY_*`), each skipping
  some passes at random (35%, 15% and 20% of the time), over the steady loops. The
  demo played the loops alone, so the Insecticons hummed where the game's buzz and
  flutter. They now play as the game's cue does, while the Insecticon hovers (the
  loops are the engine's, as before). Other Insecticon sounds checked against the
  game: their engine, boost start and boost loop (`VEH_INSECTICON_*`), their climbs
  (the jets' `VEH_JET_01_HOVER_FLIGHT_*`), their own transformations
  (`SHARPSHOT_` and `KICKBACK_BOT_TO_VEH` and `_VEH_TO_BOT`) and the start of their
  hover (`VEH_INSECTICON_FLIGHT_START_ONESHOT`) all play as before. Two fixes from
  their audio components: Kickback's has no sound for his hover stopping (the demo
  played `VEH_INSECTICON_FLIGHT_STOP_ONESHOT` for him; Sharpshot keeps it), and no
  jet's but Vortex's plays a sound when a dive stops (the demo played
  `VEH_JET_01_HOVER_FLIGHT_DESCEND_STOP` for all of them: their sound set lists it,
  the game never plays it). (`src\main.rs`, `src\sound.rs`, `src\character.rs`; the
  tests `the_insecticons_play_their_own_sounds` and
  `a_held_cues_one_offs_play_again_each_time_round`.)
- **The Hover Slam lands with the game's ground smash.** The slam's landing only
  threw up a few sparks: the demo played a vehicle's heavy landing there
  (`Vehland_High_Lrg_FX`) for want of the game's, which comes from the clip the
  game plays as he lands (`Melee_HoverGroundSlam_Recover`, the Destroyer's and
  Titan's `Shared_Truck_ROBO_ANIM` in `mp_shared.xxx`). As that clip starts it plays
  the ground smash on his `RootSocket` (`HighLeap_GroundSmash_FX`, the effect the
  Leapers' own smash plays: a glow on the ground under him, columns and a ring
  of thick smoke thrown up round him, a spray of sparks and the air bending round
  it) and shakes his view hard (its camera shake: intensity 2.5, rolling 1.75 and
  falling off over a second, the shake over 1.15 s). The slam now lands with both,
  the smash for every player in the match, the shake for the one who slams. The
  smash is on his mesh, as the clip's notify puts it, so its smoke is lit by the
  light where he lands, the tan cloud of the game's videos (as the dust of his
  landings is: `src\movefx.rs`).
  The smoke itself was missing from the demo's build of the effect: its material
  (`SmokeThick_LIT_Mat`) uses two bits of shader code the material translator had
  no translation for (`tools\ue3\fxmat.py`), so the emitters drawn with it were
  left out. With them translated the effect has its smoke, and so do the other
  effects that use that material: the Leapers' leap and blast dust
  (`HighLeap_GroundSwell_FX`, `Aoe_LeaperBlast_FX`), the Guard's blast
  (`EMP_Pulse_FX`), the enemies' jump dust (`AI_Jump_FX`), the large death
  explosion (`Exp_Death_Lrg_FX`) and the catwalk debris (`Catwalk_Debris_FX`).
  The smash's flash on the ground is drawn far dimmer than its data has it: its
  colour starts 50 times over (4, 4, 2.5, scaled by its `ColorScaleOverLife`),
  which the demo drew as a white sheet across the floor for a third of a second,
  bright enough to hide what was round him, where the game's videos show a glow
  under the dust for two or three frames. Its starting colour is taken down to
  0.4% (`tools\build_weapon_fx.py`'s `DIM_SMASH_FLASH`, its `dim` option), a
  warm glow at his feet that is gone in a tenth of a second; the rest of the
  effect is as the game has it.
  (`src\hoverslam.rs`, `src\movefx.rs`, `tools\ue3\fxmat.py`,
  `assets\fx\weapons_fx.txt`, `assets\shaders\levelfx\weapons.wgsl` and
  `assets\shaders\levelfx.wgsl`, rebuilt by `tools\build_weapon_fx.py`.)
- **An enemy's melee blow staggers you, and your gun stops till you recover, as in the
  game.** A blow from a Scrapper's fists (and from the soldiers' and the
  Shotgunners' melee) only played a short flinch, and you could shoot straight
  through it. In the game, the one struck plays his melee set's hit reaction
  (`TnMeleeManager.ServerProcessMeleeHit` and `PlayHitReaction`: every Escalation
  character's set is `TR_Shared_MS_p.SharedMeleeSet`). It picks one of three by
  where the blow came from: from ahead and to your left, ahead and to your right,
  or from beside or behind you (`mp_shared.xxx`'s `HitReactions_ROBO_ANIM`:
  `ADD_Hit_Reaction_Melee_FL`, `_FR` and `_B`, about a second each). It throws your
  upper body and arms about over whatever you are doing, your legs going on as they
  were. Escalation's reactions aren't additive in the melee set (multiplayer's
  are), so the game's melee manager goes into its `Reacting` state, which turns
  your shooting, reloading and class ability off until the reaction completes,
  0.83 s after the blow (the clip's 0.2 s blend-out from its end). Holding the
  trigger, your gun stops and fires again as you recover
  (`TnPlayerController.CheckDeferredFire`). Another blow starts it over; your own
  melee or a dodge cuts it short, as the game's do. You don't react while you
  swing your sword or punch, spin your Whirlwind, dodge, change form or are down,
  in your vehicle, or with your Flak Shield up (the reactions need it down). The
  other players see your reaction too. A Leaper's smash (its own blow, not a melee
  set's) still plays the old flinch. Checked on your PC with a Scrapper hitting
  Jazz while he held the trigger: his shots stopped at each of its three blows and
  started again 0.83 s later. (`src\hitreact.rs`, new; `src\main.rs`,
  `src\abilities.rs`, `src\emp.rs`; the clips are in `models\robot_extras.glb`, from
  `tools\build_robot_extras.py`, which `extract assets.bat` now gives
  `HitReactions_ROBO_ANIM`; the tests `a_melee_blow_staggers_him`,
  `the_reaction_is_the_side_struck_from` and
  `a_reaction_holds_his_gun_till_it_completes`.)
- **The unlocks can't be changed by editing `customize.txt`.** What each weapon,
  upgrade, ability and part needs (its level, Prime Mode) and what it costs come
  from `assets\customize\customize.txt`, which the demo read as it started: a
  level set to 0 there opened that option at once. The file is now built into the
  program as it is compiled, and the copy in `assets` isn't read at all, so editing
  it (or deleting it) changes nothing; it can be left out of what you give
  players. With your progress signed (`progress.txt`) and saved builds checked
  against your levels as they load, this was the last plain-text way to open
  things early. `tools\build_customize.py` writes the file as before: after running
  it, build the demo again (`cargo build --release`), which cargo does by itself
  when the file has changed. To try locked options while testing, edit the file
  and build, then put it back and build again. As with the progress file, this
  stops changes in a text editor, not someone who builds the demo from its source.
  (`src\customize.rs`, `src\menu.rs`, `src\upgrades.rs`; the test
  `an_edited_customize_txt_opens_nothing_early`.)
- **The maps' parts that move by themselves in the game now move: Ignition's two
  turbines spin and Oblivion's machines' lights pulse.** The map data marks the
  actors that turn on their own (their physics is `PHYS_Rotating`, with a rate to
  turn at) and the glows that pulse (the environment material's
  `Enable_Emissive_Pulse` switch, with a rate in cycles a second); the demo drew
  both standing still. A new step of `extract assets.bat`, `tools\build_motion.py`,
  lists them for each map in `assets\levels\<map>_motion.txt`, and the demo turns
  and pulses them as the game does:
  - **Ignition**: the two turbines over the upper floors of the rooms left and right
    of the players' start each turn once every 6 s about their upright axis, the two
    turning opposite ways (one is a mirror image of the other in the map). They are
    left out of the map's joined meshes so they can turn; nothing collides with them,
    as in the game.
  - **Oblivion**: the lights on the big machine ahead of the start, the orange bars on
    the ordnance feeder's doors (`OrdinanceDoor_MATINST`) and the blue ones on its pipes
    and pillars (`ArkDrivingPipe_MATINST`, `ArkDrivingPillar_MATINST`, and the same
    pipes all over the map), pulse together once a second: at full glow for a third
    of each second, then fading out to dark and back, as the material's sine-based
    pulse (`ENV_Emissive_MF`) has it, which is what your video shows (dark at about
    1.45, 2.45 and 3.45 s). Its warning signs pulse every 2 s.
  - **The other maps** get what their data has too: Downfall's map room monitors
    turn slowly, each at its own speed, and the glows the game pulses there and on
    Ancients (on beams, pillars, bridges and some of the ground, mostly one and a
    half times a second) pulse; the upgrade consoles' lights on every map pulse
    every 2 s and the storage pods' about every 3 s.
  Checked on your PC: Ignition's turbines turn in a held view, and Oblivion's
  machine, seen from in front of it, goes from full glow to dark and back each
  second, its console's steady light staying as it is. (`src\mapmotion.rs`, new;
  `src\merge.rs`, `src\main.rs`; `tools\build_motion.py`, new, and
  `tools\extract_game.py`; the tests `the_turbines_turn_about_their_upright_axis`
  and `the_glow_pulses_as_its_material_has_it`.)
- **Cars, trucks and tanks sit on the ground under them, pitching up and down
  slopes as they drive.** The demo drew every vehicle level, whatever it was on: up
  Ancients' dunes the car stayed flat, its nose in the sand and its back in the air.
  In the game each is a rigid body on four springs (each wheel's
  `TnWheelAssembly`, or hovering its hover's four springs round its centre of
  mass, `TnHoverCarSimulation` and `TnHoverTankSimulation`), each finding the
  ground under it with a trace, so the body tips to the ground as they press on it.
  The demo now does the same: each frame it finds the ground under each of the
  vehicle's four springs (on its wheels where its wheels are, hovering where its
  hover's springs are), lays a plane through those points, and pulls the body's
  pitch, lean and height to that plane with the springs' own strength and damping,
  worked out from the game's numbers (each wheel's `Stiffness` and `Damping` against
  the vehicle's `Mass` and `InertiaTensor`). Up the dune east of Ancients' start,
  about 11 to 14 degrees, the cars, trucks and tanks now ride 10 to 14 degrees nose
  up and level out over the top, on their wheels or hovering. How far a vehicle
  leans *across* a slope follows the game's physics too, and differs by form:
  - **on its wheels** a car or truck leans about a quarter of the slope, because
    the game's banking (`TnCarSimulation.UpdateBanking`, which leans it out of
    turns) also pulls it towards level, against its springs;
  - **hovering**, a car or truck leans about a seventh of it, because its hover
    turns it back towards level by 5% a frame (`TnHoverCarSimulation.UpdateTurn`);
    its pitch is left free on the ground, so it still follows the slope fully;
  - **a tank** (and Grimlock) follows the ground's lean fully: its hover holds
    neither its pitch nor its lean while it stands steady on the ground
    (`TnHoverTankSimulation.UpdateTurn`).

  In the air nothing changes: a hovering vehicle rights itself and one on its
  wheels keeps turning the way it was, as before. The jets are unchanged. The
  numbers come from a new `vground` line in each class's file, which
  `tools\build_class.py` now writes (`--ground` adds it to a class already built:
  this update's `infiltrator.txt`, `destroyer.txt` and `titan.txt` have it). A
  tank's springs use the hover car's strength, because the game takes a tank's mass
  from its physics asset rather than its blueprint. Checked on your PC with
  Jazz's car (boosting and hovering), Optimus Prime's truck and Megatron's tank
  driving up that dune. (`src\car.rs`, `src\drive.rs`, `src\character.rs`,
  `src\main.rs`; `tools\build_class.py` and `tools\class_data.py`; the
  tests `the_body_lies_on_the_plane_under_its_springs`,
  `hovering_it_pitches_to_the_ground_and_holds_its_lean`,
  `jazz_car_leans_a_quarter_into_a_side_slope` and
  `every_ground_vehicle_has_its_springs`.)
- **A headshot kill shows the headshot icon in the kill feed.** The game's death
  message for a headshot is its own line, `TnDeathMessage`'s `HeadShotString`
  ("`k [Headshot] `o"): the killer's name, the crosshair icon in place of the
  weapon's, the victim's name ("Default [crosshair] Soldier" in your screenshot).
  `TnGame.GetDeathMessageSwitch` picks it when the victim's last blow struck its
  head (`WasLastHitHeadshot`: a bone in its `HeadShotBoneNames`) and the damage
  type isn't one of those `ExemptFromAccuracyKills` (melee blows, burns, the
  Kamikaze Mines and the trap sentries' rounds), before it would pick the damage
  type's own line. The demo now does the same: an enemy whose killing blow was a
  round on its head (the same test its headshot deaths and the headshot voice
  lines already use) reads as your name, the crosshair and the enemy's name, in
  your colour. Online, each kill feed line carries whether it was a headshot, so
  every player sees the icon (the network version goes up to 41: everyone in a
  match needs this update). Checked on your PC with the HUD showing a rifle kill
  and a Sniper Rifle headshot one above the other. (`src\killfeed.rs`,
  `src\net.rs`, `src\enemies.rs`; the tests `a_headshot_reads_as_the_game_has_it`
  and `a_kill_by_a_round_to_the_head_is_a_headshot`.)
- **The Diffraction Barrier's outline is red for the Autobots.** Optimus's barrier
  (and every Autobot Destroyer's) had its hexagons in red but its outline in purple.
  The game colours the barrier by setting its materials' `EnergonColor` colour
  parameter to the owner's side's energon colour (`TnBarrierSpawnable.Initialize`).
  The outline's material (`TR_Barrier_EdgeGlow_MAT`) also has a number parameter
  of the same name, and the demo's Autobot and Decepticon versions of it set that
  one, which the material never uses, so the outline kept the material's own blue
  whatever the side. They now set the colour parameter: the Autobots' outline is
  their red (1.25, 0.05, 0.05), like the hexagons, and the Decepticons' is their
  purple (0.2, 0.1, 1.5), a little redder than the blue it had before. Checked
  on your PC with Optimus raising the barrier on Ignition. (`tools\levelfx.py`;
  `assets\shaders\levelfx.wgsl` and `assets\shaders\levelfx\weapons.wgsl` patched to
  match what it now builds.)
- **The Cloak's lightning is toned down to the game's.** As the Infiltrator cloaked
  or uncloaked, the demo threw a cage of arcs several metres long round him, crossing
  the whole screen, with a purple haze over everything near him; in the game's
  multiplayer a few thin arcs flicker about his body and his feet. Two things were
  wrong. The game plays the lightning (`Cloaking_FX`) attached to his robot mesh
  (`TnBuff.PlayEffect`, with a scale of 1 of its own), and his robot mesh is drawn
  at his robot's size (`RobotMeshMergeScale`, 0.6 for the Infiltrator), so the
  effect is drawn at that size too: its arcs, sparks and glow 0.6 as big and kept
  0.6 as close to him. (The game's own scripts count on this: its guns' effects undo
  their gun's scale, `TnWeaponEffectPlayFXOnSocket.CalculateRelativeScale`, and a
  buff's effect on a vehicle is given the robot mesh's scale, `GetVehicleFxScale`.)
  The demo drew it at full size. And the effect keeps no more arcs and sparks out at
  once than the game makes room for (its second sheet of arcs asks for 60 to 120 a
  second and keeps 10), as the shotguns' and the dash dust's effects already do.
  Checked on your PC from the player's camera as the cloak goes on and comes off,
  side by side with your video. (`src\cloak.rs`, `src\levelfx.rs`; the test
  `cloak_lightning_keeps_to_its_peaks`.)
- **The other effects the game attaches to a robot are at the robot's size too.**
  Every buff's effect goes on the robot mesh the same way as the Cloak's lightning
  (`TnBuff.PlayEffect`), and so does the Heal Beam's reward (`TnAbilityHealBeamBase`
  plays it on the healer's mesh), so the game draws them all at the robot's size: 0.6
  on the Infiltrator, 0.7 on the Scientist, 0.8 on the Destroyer and 0.9 on the Titan.
  The demo drew them at full size. Now sized: the Intercooler's lightning over
  everyone it boosts (`Intercooler_FX`: crackling a metre or so about his body, as
  in your Intercooler video, where it had thrown arcs out across half the screen),
  the Energon Recharger's glow and crosses (`KillStreak_HealthRegen_FX`), Speed
  Redirector's trail (`SpeedBoost_Trail_FX`), the flame jets' fire on a player
  (`Flamethrower_Impact_FX`), the Heal Beam's effect on the one it heals
  (`HealAbility_Beam_FX`, at that one's size; the beam still reaches the healer's
  hand) and the juice's rewards at the healer's hand (`HealBeacon_FX`, `SelfHeal_FX`,
  `HealDoubleDmg_FX`). The Intercooler's blast as it is used stays at full size for
  now: the game plays it on his mesh too, but in your Intercooler video its sphere
  sweeps out past the camera, further than the demo's does even at full size, so
  shrinking it would take it further from the game. Checked on your PC with the
  Intercooler, the Energon Recharger and the Heal Beam, before and after, the
  Intercooler beside your video. (`src\character.rs`'s
  `Robot::effect_scale`, used by `src\cloak.rs`, `src\equipment.rs`, `src\emp.rs`,
  `src\traps.rs` and `src\healbeam.rs`.)
- **Cars, trucks and tanks play their boost sound for as long as they boost.**
  Boosting, a vehicle played the short burst as the boost began (`Auto_Boost_Start`)
  and then only its ordinary engine sound, so with most of them the boost was quiet
  or not heard at all, as in your video. In the game, while a vehicle boosts its
  audio component (`HmPlayerVehicleAudioComponentImpl`'s `Boosting` state) plays its
  boost loops (`BoostSounds.BoostLoops`: `Auto_Boost_Loop` and
  `Auto_Boost_Loop_Exhaust`) in place of its engine's, and as it starts to boost on
  the ground its wheels' sound (`Auto_Boost_Wheels`, stopped once it has been off the
  ground for a quarter of a second). The demo had never taken those three sounds out
  of the game nor played them. `tools\build_sounds.py` now takes them (21 cues and 13
  new sounds, for the 25 cars, trucks and tanks that have them) and the demo plays
  each vehicle's own while it boosts, its engine's loops going quiet: Soundwave's
  truck its turbo whine and engine roar (`VEH_TRUCK_02_BOOST_LOOP`), rising over the
  first seconds, with its gears changing and its air brakes hissing at 7.5, 16, 27
  and 38 seconds as the game's cue has them; Megatron's tank its own
  (`VEH_TANK_01_BOOST_LOOP`); Jazz's car its sports car's. Grimlock, whose audio
  component plays none, is as he was. Not yet heard on your PC. (`src\main.rs`'s
  `engine_sound`, `src\sound.rs`'s `hold_through`, `src\character.rs`;
  `tools\build_sounds.py`; `assets\sounds\sounds.txt` and 13 new `.ogg` files; the
  tests `a_cue_held_through_plays_its_one_offs_once` and
  `the_ground_vehicles_have_their_boost_loops`.)
- **Hovering vehicles no longer sink into the floor.** Since vehicles began
  following the ground (above), a hovering truck or car could sit low with its hover
  jets in the floor, as in your picture of Soundwave's truck. Under many floors there
  is lower ground beside the floor a vehicle stands on: Ignition's and Oblivion's
  floors are raised plates and walkways with the floor 10 to 30 cm lower between and
  beside them, and a truck's hover springs, nearly as wide as the truck, reach past
  their edges. The plane through the four springs then lay below the plate the
  vehicle stood on, and the body settled onto that plane, into the plate. Measured at
  every flat spot of Ignition's navigation, Soundwave's truck settled more than
  10 cm below where it stood at over half of them (15 cm, as high as it hovers, at a
  third). In the game the vehicle is a rigid body: its springs push it up and tip
  it, but nothing of it goes into the floor. The demo now does the same: the body
  still pitches and leans with the plane through its springs, but settles no lower
  than keeps its hover jets (or wheels) over the ground under them and its middle
  over the ground where it stands, and its rigid body's box (its `vhull` line) over
  anything under its nose, tail and sides (a truck's or tank's box is about 45 cm
  over its hover jets, so this is mostly the cars). At those same spots the truck now
  never settles more than 10 cm below where it stands, and the cars dip their bodies
  into the floor at fewer spots than before vehicles followed the ground. Not yet
  checked on your PC. (`src\car.rs`'s `ground_under` and `GroundFit::floor`, `src\main.rs`;
  the test `the_body_rests_on_what_is_under_it`.)
- **The cars' and trucks' wheels no longer sink into the floor.** The fix above
  kept the body and its hover springs over the ground, but not the tyres, which
  still went into the floor everywhere, parked or driving, as in your pictures of
  Jazz's car on Downfall. Hovering, a vehicle's clips turn its wheels flat under it,
  and tipped like that they reach 5 to 37 cm under the body's origin, by vehicle and
  wheels (Jazz's about 13 cm, Bumblebee's up to 26, Breakdown's up to 35,
  Soundwave's truck's up to 37), while the hover
  lifts the body only 1.5 cm (a truck 15.5): the bottom of every wheel was in the
  floor. A few reach under it even on their wheels (Breakdown's and Soundwave's by
  up to 9 and 10 cm). The demo now works out each frame how far each tyre reaches
  under the body as its clips pose it (the lowest edge of its disc, however it has
  turned on its axle and tipped) and draws the body that much higher, easing to it
  in about a tenth of a second, so the body rises as the wheels turn flat and
  settles as they come back down. Folding up, the car comes down onto its tyres as
  it settles (only as far as it hasn't already settled to its hover), and unfolding
  starts from where the car was. Jets and the walkers are as they were. Checked on
  your PC at Downfall's start, from the side, before and after: parked, Jazz's car
  now floats with its wheels on the floor (its body 14.5 cm up instead of 1.4),
  Breakdown's 33, Bumblebee's 27 and Soundwave's truck's about 45 (it bobs with
  its wheels' hover clip); driving ahead; folding up (Breakdown, Soundwave); and
  unfolding (Jazz, Soundwave). (`src\car.rs`'s `tyre_depth` and `car_state`; the test
  `a_tyre_reaches_under_the_car_as_its_disc_does`.)
- **The frame rate holds while players are dead and spectating.** In your
  four-player game the frame rate dropped once a player died, and further with each
  one dead. A dead player's body dissolves over 3 seconds and then lies there, unseen,
  until the round ends (Escalation brings the dead back only then). The demo kept
  writing the dissolve's state to every one of the body's materials every frame,
  long after it had finished, and each material written is prepared for the
  graphics card all over again. That cost every PC about 16 ms a frame for each dead
  player (the dead player's own PC and everyone else's copy of him alike), so three
  dead added nearly 50 ms to every frame. The dissolve is now written only while it
  is still going (or the body still moving), and nothing at all once the body has
  dissolved. Profiled on your PC on Downfall, a dead player spectating his
  teammate: the frame's work was 28 ms alive, 45 ms dead before the fix, and 29 ms
  dead after it (the same with another player's dead body beside a living one: 44
  ms before, 28 after). (`src\downed.rs`'s `play_deaths` and `dissolve_moved`; the
  test `a_dissolved_body_writes_its_materials_no_more`.)
- **A player comes back after dying in his own paint, not as the dissolve's glowing
  lines.** In your video the Scientist came back after his death drawn only as the
  cyan lines of the dissolve that takes a dead body away, and stayed so for the rest
  of the match. A player's body starts to dissolve when his death says, or 4.5 s
  after he died at the latest (the demo cuts longer deaths short). Some deaths start
  their own dissolve after that: killed by a tank's shell (5.6 to 6.6 s), some
  electrocutions (5.9 s), gravity (8.1 s) and being crushed (12 s). When one did,
  the demo dissolved his body a second time. That turned the body visible again
  for a moment, but worse, it noted the dissolve's own materials as his "real" ones,
  and those are what he was given back when he respawned. Now his body
  dissolves once, the cut-short dissolve running on to its end whatever his death does
  after (without a second dissolve sound), and he comes back in his own materials.
  Nothing about it was the Scientist's own: any class killed one of those ways did
  the same, on every PC. Checked on your PC with Vortex killed by a tank's shell
  and brought back 11 s later, before (only the dissolve's glowing specks where he
  stood) and after (himself). (`src\downed.rs`'s `play_deaths`, `deres_of` and
  `dissolve_begins`; the test `a_death_cut_short_dissolves_once`.)
- **Ancients' Swarmers come up out of the ground, as in the game.** In your video the
  Insecticon Swarmers appeared standing in the middle of the room, with no animation.
  The spawn points the waves bring them in at there (31 of them, every one the
  Swarmers use) name an animation the enemy comes in with (their
  `SpawnAnimationName`, `Nav_Spawn_Ground`), and the demo played none but Ignition's
  Leapers' leap. The Swarmer's animation sets have it (`Swarmer_ALT_ANIM`): it
  bursts up out of the floor and gets to its feet over 1.2 seconds, with its burst
  heard (`SPAWN_WALLBUST`), its get-up chirp and its footsteps. Now an enemy that
  comes in at a point with an animation its clips have plays it there, on the floor
  under the point, its AI asleep till it is up (`TnHtnAiPawn.StartSpawnAnimation`);
  online the others see it as the host plays it. `tools\build_ai_anims.py` now takes
  the spawn points' animation into the sets that have it (`SPAWN_CLIPS`: only the
  Swarmer's), and `tools\build_sounds.py` its sound with it. Left out for now: the
  goo the game sprays as it bursts out (`Spider_Birthing_Goo_FX`), which the demo
  drew as a huge glaring purple-white puddle, ten metres and more across, that I
  couldn't check against the game. Checked on your PC on Ancients with only Swarmers
  in the waves, the room with the floor holes open: each one's legs poke out of the
  floor, it climbs out, rears up and crouches, then goes for Jazz. (`src\waves.rs`'s
  `SpawnPoint::anim` and `WaveData::spawn_anim_at`, `src\ai\mod.rs`'s `attach`;
  `tools\build_ai_anims.py`; `assets\models\enemies\ai_swarmer.glb` and
  `ai_anims_swarmer.txt`, `assets\sounds\sounds.txt` and 9 new `.ogg` files; the
  test `enemies_come_in_with_their_spawn_points_animation`.)
- **The Photon Burst Rifle's muzzle flash is the game's size again.** In your video
  each round threw out long, fat white bars of light, several metres across the
  screen, where your recording of the game shows a white-blue starburst at the
  muzzle with a few hair-thin streaks. The flash's streaks (`Streak` and
  `Streak_Dup`, material `Streak_horz_mat`) draw a texture called `Streak_CLR`, and
  the game has two of that name: `FX_OS_Common_Textures_p`'s, a thin line across a
  512 by 256 image, and `FX_Textures_p`'s, a broad bar filling most of a 256 by 64
  image, which the tank shell's tracer draws (`Tank_Tracer_MAT`).
  `tools\levelfx.py` wrote each texture once by its name, so the streaks got the
  tank tracer's bar: every streak was a solid bar the full width and nearly the
  full height of its quad, giving off about twenty times the light of the game's
  line, and its glow hid the starburst. Now the streaks draw their own line. The starburst,
  streaks and sizes were right; nothing else about the flash has changed.
  `tools\levelfx.py` now gives a texture its own file when another of the same
  name, with other pixels, is drawn in the same effects (copies of one picture
  still share one). That also corrects five of the lightning materials, which drew
  the wrong one of the game's two `Noise_CLR`s: the E.D.K. TechVolt's beam
  (`FX_TechvoltBeam_Mat`), `LightningBeamTrail_MAT`, `Lightning_Anim_01_MAT`,
  `Lightning_Panning_Chargeup_MAT` and the Snipers' tracer
  (`Tracer_Sniper_AI_MOD_MAT`). Their noise only sets how they flicker, so expect
  them to flicker a little more, not look different. Checked on your PC on
  Downfall, firing from the hip with the Scientist's armour: the starburst at the
  muzzle with thin streaks, as in your recording (from the test's camera his
  forearm hides part of the starburst, so its size isn't compared side by side).
  New in the assets:
  `textures\levelfx\Streak_CLR__FX_OS_Common_Textures_p_Textures.ktx2` and
  `Noise_CLR__FX_OS_Common_Textures_p.ktx2`, and `fx\weapons_fx.txt` pointing at
  them. (`tools\levelfx.py`'s `texture_stems`; the test
  `the_photon_streak_draws_its_own_texture`.)
- **Heavy Gunners and Titans no longer flip over and float.** In your video a Heavy
  Gunner tipped onto its side and hung there in the air for a moment. It happened
  each time one of them turned on the spot, in a fight or not. The game plays a
  quarter-turn animation over the idle for that (`TnAnimTurnInPlace`:
  `Nav_Gun_IdlePivot90_L` and `_R`, the Titan's set, which both Heavy Gunners use).
  The demo's AI picked the turn, but its model had nothing to play it on: the
  animation graph gave the base layer only the stance blend's, the moving and idle
  clips and the jumps' and falls' clips, not the turns. So for about half a second
  the base was empty under the aim poses. The aim poses only add a difference on
  top of what is under them, so here they set the spine and arms from nothing:
  the body lay over 85 to 120 degrees from upright at hip height until the idle
  blended back in. Now the turns play in the base over the idle, as in the game.
  The Swarmers' turns on the spot (`Nav_IdlePivot90_L` and `_R`) were left out the
  same way and now play too. Checked on your PC on Downfall, a Heavy Gunner and a
  Titan each brought in 15 m from Jazz and shelled at their feet: before, each one's
  first turn laid it on its side in the air (its spine logged at 86 to 122 degrees
  from upright); after, it stays on its feet through every turn. (`src\ai\graph.rs`'s
  `base_clips`; the test `a_turn_in_place_plays_on_the_model`.)
- **Controller players see the controller's button at doors and other things to
  use.** With a controller, the line at a door's console said "Press E to deposit",
  the keyboard's key. The same went for an Armory Recreator's line and for reviving
  a teammate. Every one of those lines takes its key from the weapon pickups' code,
  but that code only updated the key while a weapon or something for sale was in
  reach. Until a controller player had stood at one, the line kept the keyboard's
  E. Now the key is updated every frame. The line also shows the button the way the
  game does: the X button's picture in the text, 29 pixels high, where the game's
  key binder puts `{ButtonX}` and the line's text field swaps in the picture
  (`ButtonIconData.CreateImageSubstitutionArray(29, 18, 1)`). Lines for buttons
  the HUD has no picture of (the D-pad's left and right for equipment, the left
  stick for a sticky rocket) still show the button's name. Checked on your PC on
  Downfall at the Defensive Sentries console, with a controller plugged in and
  chosen as the controls: before, "Press E to deposit"; after, "Press [X] to
  deposit", the picture in the line. On the keyboard it is "Press E", as before.
  (`src\main.rs`'s `weapon_pickups`, `src\hud.rs`'s `command_text`,
  `BUTTON_MARKUPS`, `lay_out` and `text_icons`; the tests
  `the_interact_key_is_shown_with_nothing_to_pick_up` and
  `a_controllers_prompt_shows_its_button`.)
- **Boosting no longer slows the game down.** In your video the frame rate fell about
  five seconds into a boost, as the camera dipped under the car and its boosters'
  streaks filled the screen. Each exhaust's booster (`Booster_Jazz_FX`, Jazz's car's
  and Optimus Prime's G1 truck's) is three cone meshes that ride on the exhaust,
  each spawning 10 to 20 a second and living 20 s; the game keeps 2 of each out at
  once (their emitters' cooked `PeakActiveParticles`). The demo kept every one, each
  new cone exactly on the last, so they piled up: about 15 more of each a second, 75
  of each five seconds in, some 450 over the two exhausts, every one drawn with its
  heat haze over the whole picture behind it. Now the boosters keep to the game's 2,
  as the shotgun's and Scatter Blaster's effects, a tank shell's blast, the landings'
  dust and the Cloak's lightning already do (`PEAK_CAPPED`). The copies all lay in
  one place, so the boost looks much as it did, its streaks a little fainter and
  shorter where the pile had built them up. Checked on your PC on Ancients with
  Jazz's car boosting over the big dune, the camera behind and under it: 28 of each
  cone out after two seconds before, 2 after. (A short boost
  there didn't drop the test's frame rate either way; the slowdown needs the pile-up
  of a longer boost, as in your video.) (`src\levelfx.rs`'s `PEAK_CAPPED`; the test
  `a_cars_boosters_keep_to_their_peaks`.)
- **Letting go of the boost no longer swings the car round.** In your video, after a
  boost with a jump the car turned at once to face the crosshair the moment the boost
  was let go. Hovering, a car faces the view (the game's hover physics turns it to
  the view's yaw in a frame). Boosting, the demo's camera came round after the car
  at its driving strategy's `MatchRotationSpeed`, a third of a second behind, so
  after a hard turn or a turn in the air the view was well behind the car, and
  letting go turned the car to it. The game's driving camera does otherwise
  (`TnDrivingOrbitRotationCameraBehavior.Update`): the view's yaw is the car's own,
  and `MatchRotationSpeed` is only for its pitch, so the car stays pointed where it
  was going as it drops back to hovering. Now the view turns with the car while it
  boosts. Its smoothing still lets the car swing a little across the picture in a
  turn, though less than before, as the camera keeps closer behind it. For a moment
  after the boost the car faces the view's own turn rather than the smoothed
  picture, giving way to it as the camera blends back into hovering (1.5 s for
  Jazz's car), so it keeps its heading as the camera comes round behind it. Checked
  on your PC on Ancients' big dune: Jazz's car boosting, jumping at 2.2 s, steered
  hard right and turned through 84 degrees, the boost let go at 3.6 s. Before, the
  car turned 41 degrees at once as it let go, back to where the camera had been
  left. After, it kept its heading (6 degrees before and after) with the camera
  behind it. (`src\vehicle_cam.rs`'s `Rig::frame` and `Rig::facing`, `src\main.rs`'s
  `camera_follow` (`view_yaw`); the test
  `letting_go_of_the_boost_keeps_the_cars_heading`.)
- **A vehicle casts its own shape's shadow, not a circle.** In your video Oblivion's
  lights put a big dark circle round Grimlock's dino rather than its shadow. The
  shadows from the lights round a character (the game's light environments) are
  cast from capsules round its body: a robot's from its physics asset's bodies. A
  vehicle's physics asset is a single body round all of it, so the demo cast a
  vehicle's shadow from one capsule round its collision, as tall as the vehicle and
  as wide. For the dino that was a ball nearly 4 m across round its middle, which
  cast a round blot, with nothing of its head, neck or tail. Now each vehicle's
  capsules are fitted to its model once the model is in. Each bone gets the box
  round the vertices it carries. The bones start as one capsule round the whole
  model, and the demo keeps splitting off whichever bone (with all it carries)
  saves the most room, up to 16 capsules (the shadow shader's most). That leaves
  the dino's head and jaws, neck, body, legs, feet and tail segments each in a
  capsule of its own, moving with its bone as it walks. A flat piece, such as a
  car's body, becomes capsules side by side. Parts hung on the vehicle (a jet's
  wings, a tank's cannons) get their own capsules, and a car's guns cast a shadow
  only while they're out. Measured against the models themselves (the shared part
  of the capsules' outline and the model's own, out of both together): seen from
  overhead, 83 to 93 percent for Grimlock's dino, Jazz's car, Hardshell's tank and
  Jetfire's jet (its body, without its wings); from lower down, 37 to 53 degrees up,
  65 to 87 percent; Snarl's spiny dino about 66 to 70 percent from every side.
  Checked on your PC on Oblivion's start, with the old capsule (a test switch) and
  the new ones. Grimlock's dino went from 1 capsule to 16: from overhead its shadow
  had been an oval blot beside its middle, and is now its body and tail. Seen from
  behind and above as in your video, the round dark patch beside it is gone, and a
  shadow follows its body. Jazz's car went from 1 to 5 capsules with its guns away:
  its shadow now runs its full length. (`src\shadows.rs`'s `fit_car_shadows`, `fit_bones` and `box_capsules`;
  `src\character.rs`'s `HungPart`; the test `a_vehicles_shadow_keeps_to_its_shape`.)
- **The low-health effect fills the screen in vehicle mode.** In your screenshot
  the red scanlines (and the dark vignette under them) stopped short of the
  screen's edges in the car, leaving a strip round them. In the car the game draws
  its HUD squashed, 96% as wide and 90% as tall as on foot (its HUD movie's
  `updateHudOffset`), and the demo does the same. But the game's low-health effect
  is not part of the HUD movie: it is a post process over the whole picture
  (`LowHealthScreenEffect`). The demo draws it as two pictures inside the HUD, so it
  was squashed with the HUD, to about 98% of the screen's width and 90% of its
  height. Now it undoes the HUD's zoom (and the small jolt the HUD gets from a
  shot's camera shake), so it covers the whole screen in the car, on foot and while
  aiming, its scanlines 80 to the screen's height as the material tiles them. The
  HUD's own bars and counters still sit further in while in the car, as in the
  game. Checked on your PC on Oblivion's start with Grimlock's dino at 6% health
  and the shield down: before, the red stopped 37 pixels short of the top edge and
  13 short of the right at 1280 by 720; after, it reaches every edge, as it does on
  foot. (`src\hud.rs`'s `on_screen`, the low-health effect in `hud_logic`, and
  `HudStage`'s zoom; the test `the_low_health_effect_fills_the_screen_in_the_car`.)
- **The Swarmers' jog loops without a stop.** In your video the Insecticons (the
  Swarmers) kept stopping their legs as they came at you. Their jog clips
  (`Nav_StrafeJog_F`, `_B`, `_L` and `_R`) are 0.7 s long with a RateScale of 1.3,
  so the game plays a stride every 0.54 s. The exported files have that rate in
  them already: each is 0.54 s long. The demo got this wrong twice. It ran the
  stride at the clip's own speed, one every 0.7 s, and it looked the clip's own
  time up in the file. So for the last 0.16 s of every stride it asked for a moment
  past the file's end and got its last frame: the Swarmer glided on with its legs
  still, then picked up again. Now the AI plays every looping clip at its
  RateScale, as the game's animation nodes do (the jog's stride every 0.54 s), and
  the model looks up a clip's time over its rate in the file. The same fixes the
  other rate-scaled clips: the soldiers' stumbles (1.25) and fall (2), the
  Scrappers' and Shotgunners' landings, the Leaper's turns on the spot (0.9 and
  1.1) and the Guardian's investigating walk (0.9) now play at the game's speed
  without holding their end, and a stumble's movement keeps pace with it (it ran
  1.25 times ahead). Checked on your PC on Ancients with two Swarmers jogging at
  Jazz: before, their jog's time ran 0 to 0.7 s, past the file's 0.54 s for the
  last part of each stride, and two pictures 0.1 s apart in that part showed the
  same pose of the legs. After, it runs through the file's 0.54 s, 1.3 times as
  fast. (`src\ai\graph.rs`'s `file_time`, `src\ai\tree.rs`'s
  `rate_of` and the Strafers group, and `src\ai\clips.rs`'s root motion; the test
  `a_swarmers_jog_loops_without_a_stop`.)
- **One shadow in the sun, not two.** On the maps with a sun of their own
  (Downfall, Ancients and Ignition), a character standing in the sun cast two
  shadows at once, as in your screenshot of Jazz's car on Ancients: the sun's sharp
  one and a soft dark one under and beside it. The soft one is the demo's shadow
  from the lights round a character (the game's light environments, cast from
  capsules round its body), which it drew wherever the character was, sun or no
  sun. In the sun the sun's own light is far the strongest on the character, so
  its shadow is the one that shows. Now the shadow shader looks up the sun's own
  shadow map at a point just outside the character's body on the sun's side (the
  middle of its capsules, out past them by 0.3 m toward the sun): where the sun
  reaches that point, the soft shadow gives way and only the sun's shadow is left;
  in the sun's shade (indoors, under a roof, behind a wall) the soft shadow is
  drawn as before, and in the patches between it fades with the sun's shadow map's
  own soft edges. Maps without a sun (Oblivion) are as they were. Checked on your
  PC on the sunlit sand in front of Ancients' round vault door (58, 56), with the
  old way (a test switch) and the new: Jazz's car and Jazz on foot each had the
  soft dark patch round their feet and under them before, beside the sun's sharp
  shadow, and only the sun's shadow after. In the shade of the room Ancients starts
  you in, where the soft shadow is cast (0.58 dark), the pictures came out the same
  as before. (`src\shadows.rs`'s `sun_probe`,
  `assets\shaders\character_shadow.wgsl`'s `sun_reaches`; the test
  `the_sun_is_looked_for_just_out_of_the_body`.)
- **The soft shadows are the shape of the robot or vehicle casting them.** In your
  screenshots Hardshell's soft shadow, on foot and as his tank, was a round dark
  patch under him; in your video of the game each character's soft shadow is the
  shape of that robot or vehicle. The game casts these shadows (its light
  environments') as projected shadows of the character's own model: a shadow map of
  the character alone, seen from the way the light comes from. The demo cast them
  from capsules round the body (its physics shapes, or for a vehicle capsules fitted
  to its model), and a handful of soft capsules overlapping under a character comes
  out as a blob. Now each character casting one (the nearest few: 3 on Medium, 6 on
  High, 10 on Extra) has a light of its own, a spot light that gives no light, 20 m
  off the way its light comes from with its cone fitted round the character. Its
  shadow map holds that character alone: the character's meshes are put on a render
  layer of its own as well as their own, and that light's shadow sees only that
  layer. So Bevy draws the shadow map from the real model, posed as it is that
  frame, weapons and all. The shadow's box is drawn over the picture as before, and
  its shader looks each pixel up in that shadow map: shaded where the character
  stands between it and the light, as dark as before, its edge as soft as the shadow
  falls far behind the part casting it (4 cm where it touches, 6 cm more for each
  metre, at most 35 cm). The character's own surface is mostly left as it is (what
  is within a little of its capsules); a part of it more than 15 cm behind another
  (a Titan's legs under his shoulders) is shaded by it. A cloaked character casts
  none (the cloak's meshes cast no shadows). In the sun the sun's shadow is still
  the only one. Checked on your PC at Ancients' start with Hardshell on foot and as
  his tank, the old shadow (a test switch) against the new from 9 m behind and 6 m
  up. The old was a few rounded blobs off to his side with a clear round patch round
  each foot, and for the tank one big oval. The new is Hardshell's outline: his legs
  from his feet, then his body, shoulders and arm; for the tank its two cannon pods
  and its claws, each claw a thin shadow of its own. In a big wave on Oblivion at
  High, with the old shader (the test switch) and the new, a frame took the same
  12.2 to 12.9 ms; each character's shadow map took the graphics card 0.003 to 0.05
  ms, and `cast_shadows` 0.15 to 0.16 ms a frame. (`src\shadows.rs`'s
  `cast_shadows`, `lamp_for`, `assign_slots`, `tag` and `ShadowLamp`, and
  `assets\shaders\character_shadow.wgsl`; the tests
  `a_shadows_light_sees_the_whole_body` and `casters_keep_their_slots`.)
- **A Guardian coming in no longer crashes the demo.** In your playtest the players
  who had joined your game had their demos close in waves 7 and 8 on Ancients, the
  Guardians' first waves there. An online test on your PC on Ancients from wave 7,
  with both games drawing, crashed the same way 45 seconds in, on both PCs, the
  frame a Guardian came in. The new crash log (below) gave the cause: an error in
  Bevy's renderer, "called `Option::unwrap()` on a `None` value" in
  `specialize_material_meshes` (and its prepass and shadow versions). Bevy keeps a
  table of when it last set up each mesh's drawing, and it stops the program if a
  mesh on screen isn't in it. Bevy notes new meshes in the frame's last stage
  (PostUpdate), at no set point among the demo's own systems there. A Guardian's
  scanner beam is made in that stage too, as the Guardian comes in scanning. When
  Bevy looked before the beam was made, the beam was drawn that frame without an
  entry, and the demo stopped. It's a race between threads, so it depends on the
  PC and the moment: in the test both PCs crashed, in your game only the joined
  players' did (Ignition's Guardians, from its wave 9, could set it off too). Now, after Bevy's own bookkeeping each frame, the demo puts any
  mesh left out back in that table, to be set up that frame (Bevy would have
  added it a frame later). Checked on your PC with the same test and the fix: 15
  minutes, waves 7 to 13, and neither PC crashed. The demo put one mesh back each
  time a Guardian came in, 10 times on each PC (in waves 7, 8 and 10, the waves
  with Guardians), and another run showed that mesh was the new scanner beam each
  time. (`src\graphics.rs`'s `keep_meshes_drawable`; the test
  `a_mesh_left_out_of_bevys_table_is_put_back`.)
- **A crash log, a net log, and online games that wait out a long pause.**
  `crash.txt`, next to `settings.txt`, now gets an entry each time the demo stops
  on an error in its code: when (UTC), where in the code, what went wrong and the
  stack. The console window that showed the error closes with the demo, so it was
  lost before. `netlog.txt`, beside it, records online play: hosting and joining,
  who joined, the match starting, a player leaving or dropped and why, any
  silence of more than 3 s from a player or the host, and any frame over a second
  on this PC while online, each with the time and the wave. Online, a match now
  waits 20 s for a silent player or host before dropping them, instead of 6 s (the
  lobby waits 8 s, the game's own online timeout). A PC that pauses for a while (a
  slow load, a long hitch, its window being dragged on Windows) sends nothing
  meanwhile, and the others used to drop it. A dropped player whose game sends
  again is told at once that he is out, and goes to the menu with the
  connection-lost message instead of staying in a frozen match. A packet that
  can't be read no longer holds up the ones behind it until the next frame.
  (`src\main.rs`'s `log_panics`; `src\net.rs`'s `MATCH_TIMEOUT`, `note` and
  `receive`; the tests `a_match_waits_out_a_long_stop` and
  `the_logs_give_the_date_and_time`.)
- **The Swarmers climb out of the ground at every spawn point on Ancients.** In
  your video the Swarmers came in standing, with no animation. Most of Ancients'
  spawn points for Swarmers ask for their climb out of the ground
  (`Nav_Spawn_Ground`, from the Swarmers' own animation set), and there the demo
  already played it. The four points in the alcove by the AoE Slow station
  (`TnSpawnClusterSurvival_9813`) ask for `CINE_ReadytoRide_s01_Swarmer` instead,
  an animation from a campaign cutscene. It isn't in the Swarmers' animation set or
  in any set Escalation loads, so the demo found no such clip, played nothing, and
  the Swarmers there appeared standing. Now a Swarmer whose spawn point asks for an
  animation its set doesn't have climbs out of the ground with `Nav_Spawn_Ground`,
  as at the other points. Checked in an online test on Ancients with every Swarmer
  coming in at that alcove: each one started `Nav_Spawn_Ground` on the host and on
  the joined player's PC. (`src\ai\mod.rs`'s `spawn_animation`; the test
  `every_swarmer_climbs_in`, which checks every Swarmer spawn point on Ancients.)
- **The Controller Layout screen, and the game's other controller layouts.**
  SETTINGS > Controls > **Controller Layout** now opens the game's screen (the
  settings movie's `ControllerLayoutMenuProvider`): CONTROLLER LAYOUT with its
  rows, **Robot** (Default, Tactician, Striker, Charger, Southpaw, Hoarder),
  **Vehicle** (Default, Seeker) and **Thumbsticks** (Default, Southpaw; the game's
  Grimlock row, for the campaign, is left out: see above), and under them the game's Xbox 360 controller with a label
  at each control saying what it does in the focused row's form (for Thumbsticks,
  only the sticks). The controller, the buttons' pictures and the labels' lines
  are drawn from the settings movie (`mc_controllerMapXbox360`). Each label's text
  is the game's, worked out as the game works it out: its control's commands in
  the game's controller bindings with the chosen layouts applied, each named by
  the game's descriptions for that form (`GetKeyDescription`). For every layout in
  your video the labels read as the game's do. A label whose text changes comes in
  white, a little larger, and settles over half a second; one left with nothing to
  say fades out (`ControllerLabel`). With no controller connected the controller's
  place says "Controller Error", as the game's screen does. The row on the
  Controls menu is greyed out unless Active Game Controls is on Controller.
  The layouts change the controls, as the game's do (`TnControlScheme`, the same
  changes the labels read). **Tactician**: Melee on B, Switch Camera on the right
  stick's click. **Striker**: Change Form on Y, Switch Weapon on the D-pad's up.
  **Charger**: Change Form on LB, Dash/Sprint on the left stick's click.
  **Southpaw**: the bumpers and triggers swapped (LB the ability, LT fire, RB
  Dash/Sprint, RT Fine Aim, and Boost in a vehicle). **Hoarder**: Change Form on
  X, Interact/Reload on the D-pad's up (tapped it reloads, held it interacts).
  **Seeker** swaps a jet's climb and dive (B climbs, A dives; a car still jumps on
  A), and **Southpaw** thumbsticks swaps the sticks. The HUD's button prompts name
  the buttons of the layout in use. The choices are kept in `settings.txt`
  (`controller_robot_layout`, `controller_vehicle_layout` and
  `controller_thumbsticks`). Checked on your PC
  with a test that opens the screen with a controller made up: with the Tactician,
  Striker, Charger and Southpaw robot layouts, the Vehicle row and the Southpaw
  thumbsticks, the controller, its labels and their lines stood where your video
  shows them, to within a couple of pixels, saying the same things. Caught 0.03 s
  and 0.06 s into a change, the new labels were coming in white and a little
  larger, as in the video's frames, and the ones leaving were fading. With no
  controller the screen said "Controller Error". (`src\padlayout.rs`;
  `src\pad.rs`'s `lay_out`; `src\menu.rs`'s `update_pad_labels` and `layout_pad_map`;
  `tools\build_menu.py`'s `write_controller_map`, which writes
  `assets\menu\padmap.txt` and `assets\menu\img\pad_*.png`; the tests
  `the_robots_default_layout_reads_as_the_game_labels_it`,
  `the_schemes_move_their_controls`, `the_schemes_move_the_actions`,
  `the_controller_labels_change_with_the_schemes_and_the_focus` and
  `the_controller_map_reads_as_the_tool_writes_it`.)
- **The effects cost the frame less to draw.** Every emitter's particles (the
  maps' smoke, steam and fires, and every effect a gun, a blast or an enemy
  starts, with their trails, beams, mesh particles and marks) and the maps' lens
  flares were each a mesh of their own, written again every frame; every changed
  mesh is sent to the graphics card anew, its old place there freed and a new one
  found and its draw worked out again, by the render thread, which sets the
  frame's pace in a big wave (some 190 meshes a frame in the wave measured).
  Now they all draw from one buffer, the frame's effect data, written to the
  graphics card whole once a frame (about 860 KB in that wave): their meshes only
  number their vertices (a few such meshes, shared by them all and never
  changed), and the effects' shader reads each vertex from the buffer by its
  number. In the wave measured (Oblivion, wave 15, High, 1080p, on your PC) a
  frame took 12.0 ms on average before and 10.9 to 11.1 ms now (several runs);
  with the window split in two, as a split screen would draw it, 16.5 ms before
  and 15.6 to 16.1 ms now. The effects look as they did: pictures of the Riot
  Cannon's and the Chaos-Rift Combustor's shots, the E.D.K. TechVolt's beam, a
  soldier blown apart and Oblivion's exploding barrel (played the same way each
  time) were taken with both and compared pixel by pixel, and are the same but
  for a few frames where two effects start at the same spot (the shot's flash and
  its smoke, the barrel's blast and its fireball): which of those is drawn over
  the other was never fixed (they are as far from the camera), and comes out the
  other way round in some; Ignition's flame jets and the garage look the same
  too. Emitters and flares also draw before the frame's visibility is worked out,
  so one that starts drawing is always drawn that frame (it could be a frame
  late).
  (`src\levelfx.rs`'s `FxWords`, `FxDrawn`, `gather_fx_words` and
  `write_fx_words`; `tools\levelfx_shader.wgsl`'s `fx_vertex`, written into
  `assets\shaders\levelfx.wgsl` by `tools\levelfx.py`; the tests
  `effects_are_gathered_as_the_shader_reads_them`,
  `the_effect_datas_buffer_grows_with_it` and
  `effects_out_of_view_are_hidden_and_left_unwritten`.)
- **The HUD costs the frame less to draw.** Each of the HUD's pieces kept its
  Flash colour transform (what its fades, flashes and tints set) in its material,
  and a changed material is prepared again for the graphics card by the render
  thread (a buffer made anew for each of its values, and its bind group): in a
  fight about 18 of the HUD's a frame. The pieces' colour transforms are now in
  one buffer, each piece's at its own place in it (its mesh tag, given as the
  piece is made and given back as it goes), copied to the graphics card when they
  change, and the HUD's shader reads its piece's from there; a piece's material
  changes now only when its place in the drawing order does (2 or 3 a frame). In
  the wave measured (Oblivion, wave 15, High, 1080p, on your PC) the HUD's
  materials took about 0.4 ms of the render thread's frame and take next to none
  now. In one view a frame's average doesn't show it clearly (10.9 to 11.7 ms in
  four runs before, 10.9 to 11.2 ms in three now: runs differ by more than that),
  but with the window split in two, as a split screen would draw it, where the
  render thread sets the pace, a run took 15.7 ms before and 15.3 ms now. The HUD
  looks as it did: pictures of the Riot Cannon's shots and Oblivion's exploding
  barrel with the HUD (played the same way each time) are the same pixel for
  pixel, the kill feed's lines coming in the same but for a few pixels of a glow
  in the level behind them, and Downfall's sentries firing at three soldiers show
  the same HUD (its FIRST BLOOD and the rest). (`src\hud.rs`'s `HudColors`,
  `HudSlot` and `write_hud_colors`; `assets\shaders\hud.wgsl`; the test
  `hud_pieces_get_slots_for_their_colours`.)
- **No material changes from frame to frame.** Bevy keeps the standard materials
  (the maps', the models', the effects') in shared groups, and makes a group's bind
  group anew whenever one of its materials changes, comes or goes (on the maps,
  groups of many textures), as well as preparing the changed material again for the
  graphics card. Every frame the demo changed the map's pulsing glows (18 materials
  on Oblivion), the consoles' and pods' flaring outlines, the boxes the characters'
  shadows from the lights are drawn in (about 6) and every sprite of a gun's effects
  (each in a material of its own, faded as it went: 30 to 80 a frame while the
  Neutron Assault Rifle fires, up to 150 with the Scatter Blaster), so the groups
  were made anew every frame. Now the pulses and flares are worked out in the shader
  from the frame's time, in a new kind of material (`LiveMaterial`: the standard
  material's own shading, its glow scaled), which the glowing parts are given as
  their models come in; the sprites share one material for each look and take their
  colours from one buffer of the sprites' colours (by their mesh tags), as the HUD's
  pieces do; and the shadows' boxes share one material, each reading its shadow from
  one buffer of the shadows. Only the consoles' flip books still change a material,
  a few times a second. In the wave measured (Oblivion, wave 15, High, 1080p, on
  your PC) the render thread's preparing of materials went from about 1.3 ms a frame
  to under 0.1 ms, and a frame took 10.5 ms on average (10.9 to 11.2 ms in the runs
  before); with the window split in two, 14.8 ms (15.6 ms before). While the rifle
  fires (one view, no enemies) it saves 1 to 1.4 ms of the render thread's frame.
  (Bevy's main thread looks over each kind of material every frame, about a third of
  a millisecond for the new one, alongside the drawing.) The pictures are the same:
  the rifle's and the Scatter Blaster's shots (their flashes, tracers and smoke), a
  pod's outline on Downfall and Oblivion's Combustion Chamber console's, played the
  same way each time, came out as before but for a few pixels a shade or two apart
  (the glows' sines worked out on the graphics card), and a robot's shadow drawn in
  red (the shadows' test mode) exactly as before; Oblivion's pipe lights by the
  start pulse as before (where in its pulse a run starts varies, as it did).
  (`src\live.rs` and `assets\shaders\live.wgsl`, new; `src\fx.rs`'s `SpriteLooks`;
  `src\shadows.rs`'s `SlotShadows` and `assets\shaders\character_shadow.wgsl`;
  `src\mapmotion.rs`, `src\doors.rs`, `src\recreator.rs`, `src\merge.rs`,
  `src\lighting.rs`; the tests `the_glows_pulse_and_flare_in_time`,
  `sprites_get_slots_for_their_colours` and
  `slot_shadows_are_written_as_the_shader_reads_them`.)
- **Split screen's second view costs a third of what it did to draw.** With the
  window split in two (top and bottom, a second camera beside the first, as a second
  player's would be), each frame drew the game twice, and in the wave measured
  (Oblivion, wave 15, High, 1080p, on your PC) the second view added about 4 ms to
  the frame (3.4 to 4.4 ms in four runs), nearly all of it on the render thread. To
  find where, the test harness can now turn an experiment off and on in turns within
  one run (each 10 seconds of the profile one way or the other), so that both halves
  of a run see the PC as it is then (runs minutes apart differed by up to 2 ms a
  frame). Turned off that way, the depth and motion-vector prepasses, the
  characters' shadows from the lights and the HUD's camera made next to no
  difference, nor did bloom, depth of field and motion blur; the map's effects cost
  about 0.5 ms a view. wgpu's own trace of its calls (counted and timed by the
  harness, not written) showed the rest: binding the standard materials. Bevy keeps
  them bindless, in a few big shared groups of textures (up to 2048 each) that the
  shader indexes into, and each time a pass binds such a group wgpu goes over every
  texture in it, 40 to 70 µs for the maps'; the passes bound them again whenever
  another kind of material came between, most of all in the transparent pass, where
  the maps' see-through surfaces stand among the effects: some 65 times a view each
  frame, about 2.5 ms of the render thread's frame for every view. The demo now
  starts its renderer without buffer binding arrays, so the standard materials
  aren't bindless: each has a bind group of its own few textures, about a
  microsecond to bind (the light maps keep their binding arrays, which need texture
  arrays only). The second view now adds 1.2 to 1.3 ms. With the window split, a
  frame took 11.7 ms on average, against 14.8 ms in the same build with Bevy's
  settings (the render thread's work 10.9 ms against 14.1 ms), and now the main
  thread's work (about 10 ms) sets the pace nearly as much as the render thread's.
  In one view the render thread's work went from about 9.7 ms to 8.1 ms; one view's
  frames (10.2 ms against 10.7 ms) are paced by the main thread. The pictures are
  the same pixel for pixel: the rifle's shots on Oblivion, an Armory Recreator on
  Downfall and a car on Ancients, played the same way each time, with the standard
  materials bindless and not. (`src\graphics.rs`'s `wgpu_settings`, which
  `src\main.rs` gives Bevy's `RenderPlugin`.)
- **The maps are drawn as half as many meshes.** Bevy makes the bind groups of its
  light-map slots (four light-mapped meshes to a slot group) anew in every render
  phase every frame, and on Oblivion that was some 340 bind groups made and dropped
  a frame, about 1.3 ms of the render thread's work. The map's parts are joined into
  meshes that draw alike (`src\merge.rs`), but two things kept them apart: a
  mirrored part (placed with a negative scale, about a quarter of every map's parts)
  has from Bevy's glTF loader a copy of its material that culls the other side, so
  the mirrored parts made meshes of their own; and the light-map pages were on
  atlases of 16, so a material spread over two or three atlases made a mesh on each.
  Now a mirrored part is joined with its material's unmirrored parts, its triangles
  turned round (the copy matched to its twin by everything it holds, its culling
  aside; never a flip book's, as each turns its own pages), and all a map's pages go
  on one atlas (up to 49). Downfall's meshes went from 252 to 119, Oblivion's from
  199 to 115, Ignition's from 194 to 85 and Ancients' from 82 to 61. In the wave
  measured (Oblivion, wave 15, High, 1080p, on your PC, the same build joining the
  old way and the new), the light-mapped meshes went from 189 to 105, the bind
  groups made and dropped a frame from 420 to 273 and the draws from 528 to 428, and
  the render thread's work from 9.4 to 8.2 ms in one view (12.8 and 15.5 ms to 12.0
  and 14.0 ms with the window split, in two runs each); Bevy's preparing of the
  meshes' bind groups went from 1.3 to 0.8 ms. The pictures are the same but for a
  few pixels a shade apart (the light maps' new places on the atlas): the rifle's
  shots on Oblivion and a view round each map's start. (`src\merge.rs`'s
  `PartMaterial::look` and `join`; `src\lighting.rs`'s `FlipBooks::has`; the tests
  `a_mirrored_copy_looks_like_its_twin` and `mirrored_parts_join_turned_round`.)
- **An installer for players.** `make installer.bat` makes **Cybertron Escalation
  Setup**, one file for players who'd rather not install Rust, Python, ffmpeg and
  UE Viewer: it installs the demo ready to play, and as it finishes a window
  builds the game files from the player's own copy of Fall of Cybertron (the same
  build as `extract assets.bat`), showing how far it has got and about how long is
  left, downloading ffmpeg first. Closed, it carries on next time; a part that
  fails can be tried again, with its log to send. The demo itself opens that
  window while its game files aren't built (or an update changed the build's
  tools). The asset build also finds the game in any of Steam's libraries now,
  from Steam's own list of them, not only in the usual places. See *Making the
  installer for players* and *Installing the demo with its installer* below.
  (`tools\make_installer.py`, `tools\installer\foc_esc.iss` and `builder.py`;
  `src\main.rs`'s `needs_builder`; `tools\extract_game.py`'s `steam_libraries`;
  the tests `an_installed_copy_builds_its_game_files_first` and
  `tools\installer\test_builder.py`.)
- **The Customize screen glows in each faction's colours again.** The preview
  robots' energon (the Titan's leg stripes, the chest lights and the like) glowed
  cyan for both factions, as in your pictures, because the demo coloured them by
  Team Coloring Mode: with Ally vs Enemy (your setting, in the demo and in your
  game's profile) the cyan of the player's own team. The game's code does that
  (`TnCharacterApplier.GetEnergonColorForTeam`) only where it counts as a team
  game or a lobby, and its Customize screen, as your game shows it, is neither. The
  previews now glow red as Autobots and purple as Decepticons (Toggle Faction)
  whatever the Team Coloring Mode, which still colours the HUD's messages.
  (`src\customize.rs`'s `preview_glows`, `src\character.rs`.)
- **Ignition's flame jets burn steadily on Low and Medium graphics.** On Low (and
  Medium) the flame jets sputtered, a burst of fire every half second or so with
  gaps between, as in your video. The cause was the demo's own Graphics setting, not
  the game: on Low and Medium the demo uses each effect's less detailed levels
  nearer than the game would (two and one levels), and the flame jets' effect
  (`Flamethrower_FX`) has only one, which the game keeps for when it is 500 m away:
  a fifth of the flames, sparse enough to look like a stream from that far but
  broken into puffs up close. Now Low and Medium drop an effect only to levels it
  keeps for no further off than they draw effects at all (60 m and 120 m), so the
  flame jets burn whole on every setting, while effects with a nearer lower level
  (about two in five of those that have one) still drop to it. (`src\levelfx.rs`'s `lod_level`; the
  test `the_graphics_level_drops_detail_only_to_levels_seen_that_near`.)
- **A dash no longer leaves dust behind.** As a dash ended the demo kicked up a
  ring of dust under the robot (a side dash's as he touched down, a forward or back
  dash's at his feet as it ended, a Destroyer's or Titan's back dash a spray of
  sparks and dust); in the game's multiplayer a dash leaves none, only the boosters
  firing. The dust is gone; the boosters still fire, and a landing still throws up
  its dust. (`src\movefx.rs`: the dashes' `Long_Fall_Landing_1_FX`,
  `Long_Fall_Landing_Sm_FX` and `Vehland_Low_Med_FX` left out; the test
  `reads_the_table_in_clip_order`.)
- **Every sword swing lunges, with or without an enemy to lunge at.** With no enemy
  under the crosshair the swing used to stay where it was; as in your video, every
  swing in the game carries the robot forward. The game starts the swing's flight
  whatever it is aimed at (`TnMeleeManager.StartFlight`: the flight's `Direction` is
  his yaw, its `Target` the melee assist's pick or none), and with no target the
  flier flies along that direction (`TnFlier.Update`). So every swing now lunges:
  at the enemy under the crosshair as before, or straight ahead the way you look,
  18.75 m/s for a quarter of a second (4.7 m), stopping if the blow lands on
  something, after which you stand for the rest of the swing. (`src\melee.rs`,
  `src\main.rs`; the tests `with_no_enemy_the_lunge_flies_straight_ahead` and
  `a_sword_swing_lunges_at_the_enemy_under_the_crosshair`.)
- **Landing from a jump shakes the camera.** The game's landing clips each shake the
  lander's own view as they start (a `TnAnimNotify_CameraShake` on their first frame,
  played by the camera's shaker as a shot's shake is): `Nav_Land`'s and
  `Nav_Land_02`'s Intensity 0.85 and Roll 0.7, over 0.35 to 0.4 s, and
  `Nav_Land_Hard_01`'s the same over 0.45 s; in your videos the view bobs about a
  degree at four times a second for a third of a second as the Infiltrator and the
  Titan land. The demo now shakes the view as you land: after a fall of 2.5 m or
  more, by the landing clip the game's multiplayer landing table picks for it (5 m
  or more, a jump's: `Nav_Land_02`'s; a hard landing, `Nav_Land_Hard_01`'s), running
  or standing, though not as a sword swing lands nor a Hover Slam (which has its own
  shake). The take-off clip has its own lighter shake (`Nav_TakeOff_01`'s: Intensity
  0.15, Roll 0.25, over 0.3 s), which the demo now plays as you jump. (`src\main.rs`'s
  `landing_shake`; the test `landing_from_a_jump_shakes_the_view`.)
- **Every gun's muzzle flash is now the game's own.** The Photon Burst Rifle, the
  Scatter Blaster, the Nucleon Charge Rifle, the Path Blaster and the car's guns
  drew their own imitations of the game's flashes, flat sprites in place of the
  effects' meshes (crossed flames, flat discs and glows), as the rifle and the
  Subsonic Repeater did before them. Now each plays the game's own particle system
  on the barrel that fired, as the game's weapons do with every shot
  (`TnWeaponEffectPlayMuzzleFX`), with its light as before:
  `Photon_BurstRifle_MuzzleFlash_Fx` (its blue streaks, blast caps, chunky bits and
  starburst on the cap mesh, a flame and the glow), `Scatterblaster_MuzzleFlash_FX`
  (its burst of fire on the cap mesh, white-hot sparks, the smoke rolling out ahead
  and the heat shimmer the demo used to leave out; not the dark smoke, which the
  game never draws, its material having no shaders for it),
  `NucleonRifle_MuzzleFlash_FX` (up to six flame caps, a cloud of fiery smoke and
  glowing puffs) and `PathBlaster_MuzzleFlash_FX` (its white-hot flash, spinning fire
  burst and fiery iris ring on the dish mesh, and the Nucleon's three), all from
  `FX_OS_Weapons_p`; the car's guns play the vehicle rifle's
  (`AssaultRifle_VEH_MuzzleFlash_FX`, as the Battle Sentry's gun does), kept on the
  gun that fired as the jets' and trucks' flashes are. The Scatter Blaster's flash
  keeps to the game's cooked peaks as its impact and tracer already do (its smoke
  in front asks for 12 to 15 puffs and keeps 5: the game's videos show the light
  smoke that leaves); the others, like the rifle's and the Subsonic
  Repeater's, are drawn whole. The Photon Burst Rifle now has its barrel's socket
  (`RB_BurstRifleR_SKEL`'s `MuzzleFlash`, 295 units down the forearm), so its flash,
  tracers and shell port sit where the game's do. **The other guns' flashes are at
  the robot's size too**, as the game draws every effect on a gun: the Riot Cannon's,
  Thermo Rocket Cannon's, A-4 Pulsar Cannon's and Chaos-Rift Combustor's at 0.8 on
  the Destroyer, the X18 Scrapmaker's, Energon Harvester's and Corrosive Slime
  Cannon's at 0.9 on the Titan, the Orbital Pistol's at 0.7 on the Scientist, and
  the special shots' flashes the same way (the E.D.K. TechVolt's flash is part of
  its beam and stays as it was). New in the assets: the four effects in
  `fx\weapons_fx.txt`, two materials in `shaders\levelfx\weapons.wgsl` and
  `shaders\levelfx.wgsl`, and four textures in `textures\levelfx`
  (`MuzzleCap_RocketTurret_CLR`, `MuzzleFlash_Sparks_CLR`, `SpaceBridge_Iris_CLR`
  and `TankMuzzleFlash_CLR`), all made from the game by `tools\build_weapon_fx.py`.
  Without videos of these four guns in the base game they were checked against the
  game's data and renders on your PC; not yet seen in play.
  (`tools\build_weapon_fx.py`'s `SYSTEMS`; `src\main.rs`'s `muzzle_system` and
  `PHOTON_SOCKETS`, `src\fx.rs`'s `PHOTON_MUZZLE`, `SCATTER_MUZZLE`, `NUCLEON_MUZZLE`,
  `PATH_MUZZLE` and `CAR_MUZZLE`, now their lights alone, `src\riot.rs`'s
  `MuzzleFlash` and `car_gun_flash`, `src\levelfx.rs`'s `PEAK_CAPPED`; the tests
  `the_rifles_flash_is_the_games` and `the_scatter_blasters_flash_keeps_to_its_peaks`.)
- **The Subsonic Repeater's muzzle flash is the game's own.** Beside your video of
  the game, the demo's flash threw long, wide sheets of white-blue light out from
  the gun: it drew its own imitation of the game's effect, with flat sprites
  standing in for the effect's meshes (big sheets with straight edges) and an added
  glow. Now the gun plays the game's own particle system,
  `Subsonic_Repeater_MuzzleFlash_FX` (`FX_OS_Weapons_p`), as the game plays it with
  every shot, on the barrel that fired and on the gun's `MuzzleFlash2` socket: up
  to six caps of white-blue squiggles on the cap mesh, up to six Ion cores bursting
  out along the barrel, a starburst on the cap mesh and its glows (which brighten
  what is behind them, as the game's do), with its light as before: a tight,
  white-hot burst with a few rays, as in your video. As with the rifle's flash, the
  caps and cores aren't held to their cooked peak of one: kept to one the flash is
  a dim wisp beside your video. Both guns' flashes are now drawn **at the robot's
  size**, as the game draws an effect on a gun (the gun is drawn at its robot's
  size and the effect with it: `TnWeaponEffectPlayFXOnSocket.CalculateRelativeScale`
  undoes only a scale over 1): 0.6 of it on the Infiltrator, 0.7 on the Scientist,
  0.8 on the Destroyer and 0.9 on the Titan. At full size the Subsonic Repeater's
  flash was a white fog over the Infiltrator's gun; the rifle's flash is a little
  smaller than in the last update too (0.7 of it on the Scientist, whose gun it is).
  New in the assets: the effect in `fx\weapons_fx.txt`, its materials in
  `shaders\levelfx\weapons.wgsl` and `shaders\levelfx.wgsl`, the Ion core mesh
  `models\fx\Ion_Cannon_MuzzleFlash_STAT.fxmesh` and three textures in
  `textures\levelfx` (`Ion_Cannon_MuzzleFlash_Cap_CLR`, `_Core_CLR` and
  `_Distort_CLR`), all made from the game by `tools\build_weapon_fx.py`. Rendered on
  your PC on the Infiltrator and on the profile's default build; not yet seen in
  play. (`tools\build_weapon_fx.py`'s `SYSTEMS`; `src\main.rs`'s `muzzle_system`,
  `src\arsenal.rs`'s `play_flash_on_socket` and `GunFx::robot_sized`, `src\fx.rs`'s
  `SUBSONIC_MUZZLE`, now the flash's light alone; the test
  `the_rifles_flash_is_the_games`.)
- **The Neutron Assault Rifle's muzzle flash is the game's own.** Beside your video
  of the game, the demo's flash was small and pale: it drew its own imitation of
  the game's effect, three pairs of crossed flame sprites in a paler colour than the
  game's flame cap, without the black smoke caps round it. Now the rifle plays the
  game's own particle system, `AssaultRifleArm_MuzzleFlash_FX` from the vehicle
  rifle's package (`FX_AssaultRifleVH_p`; another effect of the same name,
  `FX_WPN_GUN_AssaultRifleArm_p`'s, is the Fireblast's and the enemy soldiers'
  flash), run as the game runs it on the barrel that
  fired, as the X18 Scrapmaker's is: six flame caps on the cap mesh, each turned
  its own way, blowing out into a spiky star and drawing out into a long flame
  along the barrel in a tenth of a second, the black smoke caps round them, nine
  spark bolts and the olive haze of smoke, with its light as before. Unlike the
  tracer and the steam (below), the caps aren't held to their cooked peak of one: a
  picture of the flash kept to one cap is a small single flame, and your video shows
  the six caps' star. The effect is new in `fx\weapons_fx.txt`
  (`NeutronRifle_MuzzleFlash_FX`); its materials, mesh and textures were there
  already (the Scrapmaker's flash has the same cap), so nothing else in the assets
  changes. Rendered on your PC; not yet seen in play. (`tools\build_weapon_fx.py`'s
  `SYSTEMS`; `src\main.rs`'s `muzzle_system`, `src\fx.rs`'s `RIFLE_MUZZLE`, now the
  flash's light alone; the test `the_rifles_flash_is_the_games`.)
- **The Neutron Assault Rifle's smoke and tracers are the game's.** As in your
  video, the demo's rifle wrapped itself in thick white smoke as it fired and
  reloaded, and its tracers came out as long, fat sprays that seemed to crawl. The
  game's effects hold far less than the demo drew, for two reasons. First, the dark
  smoke puffs in its muzzle flash and where its rounds strike (`Smoke_Dark` in
  `AssaultRifleArm_MuzzleFlash_FX` and `AssaultRifle_VEH_Squib_FX`) are never drawn
  by the game: their material has no shaders for the way the emitter is drawn (the
  same check `tools\levelfx.py` makes for every effect). The demo drew 4 to 5 of
  them a shot, ten shots a second, each swelling to five times its size. Second,
  the game's emitters keep no more particles out at once than their cooked peak
  (as the Scatter Blaster's and others' already do in the demo): the tracer
  (`AssaultRifle_VEH_Tracer_FX`) asks for 5 to 10 streaks a shot and keeps one, the
  steam from a spent shell's port (`Sm_Shell3_FX`) one wisp at a time where the
  demo let out 30 a shell, and the reload's steam (`ReloadSteam_Med_FX`) 15 wisps
  where the demo let out 35. Now the rifle fires one streak a shot, at the same
  speed as before (120 to 150 m/s) but alone rather than in a spreading bundle, with
  only the faint haze at the muzzle, a wisp or two of steam from its shell
  port, and a lighter cloud of steam as it reloads. The same fixes reach what
  shares those effects: the car guns' flash and tracer (the vehicle's Neutron
  rifle), and the other guns' reload steam and the Scatter Blaster's shell steam,
  each to its own peak. Rendered on your PC; not yet seen in play. (`src\fx.rs`'s
  `RIFLE_MUZZLE`, `CAR_MUZZLE`, `BULLET_IMPACT`, `BULLET_TRACER`, `SHELL_STEAM` and
  the steams' `puffs`, `src\main.rs`'s `steam_puffs` and `shell_steam_wisps`; the
  tests `the_rifles_smoke_and_tracer_are_the_games`, `steams_keep_to_their_peaks`
  and `steam_comes_out_puff_by_puff`.)
- **Optimus Prime's and Megatron's lines turn the battle down, as in the game.**
  The announcer's lines in a match still sounded quiet beside the characters'
  lines. In the game they aren't played louder: each of them (all 22 the demo plays:
  the game type, the countdowns, "Wave Complete", the last kills, an ally destroyed)
  turns on a mixer preset as it plays, `GLB_DX_DUCK_9DB`, which turns all the
  effects and the music 9 dB down (fading down over 0.15 s, and back up over half a
  second after the line), leaving every voice as it is. The demo had no mixer
  presets, so the lines played over full-volume gunfire and music. It now has the
  game's mixer: its categories (`SND_SoundConfig_p.SoundMixerProperties`, in
  `HM_Engine.xxx`: each category's volume scales those under it) and every preset the
  demo's sounds turn on, each with its fades and how long it stays on, 18 in all.
  Besides the announcer's: an ability's sounds (the Barrier going up, a heal, the
  Energon Recharger, a Sentry) turn the music and the guns 6 dB down for 0.3 s; a
  wave's starting stinger the effects 6 dB for 3 s; your own vehicle's jump its
  engine 10 dB and the end of its boost 6 dB, its boost loop the ambience; your
  own blast damage the guns 12 dB for half a second; Ignition's gunship trap the
  guns and ambience; a health pickup the combat sounds and music; a collapsing
  column the ambience and footsteps; and eight robots' shots at once the
  ambience 16 dB (the shots' category turns that one on itself). The 10% boost the
  announcer's lines had (below) is gone: they play at the game's volume, with the
  battle turned down under them. Only the presets' volumes: they also set filters,
  reverbs and pitch, which the demo leaves out. Not yet heard on your PC.
  (`src\mixer.rs`, `src\sound.rs`, `src\main.rs`'s `engine_sound`;
  `tools\build_sounds.py`; `assets\sounds\sounds.txt`'s new `mixcategory`, `preset`,
  `channels` and `mixer` lines; the tests
  `an_announcers_line_turns_the_effects_and_music_down_9_db`,
  `a_preset_with_a_duration_stays_on_that_long`,
  `presets_on_at_once_both_turn_down_and_crowds_turn_theirs_on`,
  `the_announcers_lines_turn_the_effects_and_music_down` and
  `a_cue_turns_its_preset_on_as_it_plays`.)
- **Every vehicle's camera is anchored over its middle, as the game's is, so the
  vehicle sits low in the picture.** After the change below the camera still sat
  too low behind the vehicles, the truck or jet standing up into the middle of the
  picture where the crosshair is. The game anchors a vehicle's camera on the pawn,
  and the game centres the vehicle's mesh on the pawn (`TnVehicleForm.
  CalculateCylinderBounds` moves the mesh so the middle of its bounds is the pawn's
  location; the hovering and driving cameras' anchor behavior,
  `HmOffsetAnchorPointRelativeToActorCameraBehavior`, starts from that location,
  and the flying jets' `TnFlyingFollowCameraBehavior` too). The demo had anchored
  the cars and trucks on their rigid body's bone, a few tens of centimetres off the
  ground (only the game's death camera uses that bone), the jets on their models'
  origin at the ground, and the tanks on half their height. So the camera now sits
  over each vehicle's middle and the anchor's offset on top of that, as in the
  game: about a metre higher for the trucks (Optimus Prime's truck's middle is
  1.45 m up, its bone was 0.42 m), 1.36 m higher for Starscream's jet, a few tens
  of centimetres for the cars, and a little either way for the tanks (with the
  middle a little ahead of or behind a vehicle's origin, the camera that much
  closer or further). Looking out from his truck or jet you see over its top, the
  vehicle in the lower part of the picture and the crosshair clear of it, as in
  your screenshots from the game (the demo's camera drawn over them puts the
  truck's outline on the game's truck). The aim's offsets by the view's pitch now
  run on the game's curve through their three points, not straight lines between
  them (`HmCurveVector`: Optimus's truck's aim is 0.26 m down with the view level,
  not 0.18 m). (`tools\build_class.py`'s `vehicle_middle`, writing a `vmiddle` line
  for every vehicle, the cars' and trucks' `vcam ... origin` lines at their middle,
  and `curve_at`; `assets\models\<class>\<class>.txt`; `src\character.rs`,
  `src\vehicle_cam.rs`, `src\main.rs`; the test
  `the_vehicles_cameras_are_escalations`.)
- **The vehicles' cameras are Escalation's.** In vehicle form the camera sat too far
  back and too high, making aiming hard: the demo's vehicles used the multiplayer
  classes' camera sets (`Truck_MP_CAMSET` and the like), while each of Escalation's
  characters has its own (in the `ESC_` maps' robot definitions). Optimus Prime's
  truck in Escalation (`Truck_Optimus_CAMSET`) hovers with the camera 7 m behind it
  where the demo had 9, and 0.35 m lower in the middle of its pitch range, the
  view kept within 25 degrees of level; so the truck sits lower in the picture,
  bigger, and the crosshair has more of the world in front of it, as in the game.
  Every vehicle now takes the camera set of the Escalation character who changes
  into it, and one no Escalation character drives the set most of its kind use:
  - the Destroyer's trucks: Optimus Prime, Ratchet, Soundwave and the rest
    `Truck_Optimus_CAMSET` (hovering 7 m out, 8.75 m driving), Swindle and Onslaught
    `Truck_ALT_CAMSET` (7.25 m, 8.5 m): about 1.5 m closer and 0.3 to 0.45 m lower
    hovering than before;
  - the Titan's tanks: Warpath, Megatron and the rest `Tank_Megatron_CAMSET`, Brawl,
    Quake and Perceptor `Tank_SP_ALT_CAMSET`: about where they were (8.25 and 8.5 m
    behind, 2.2 and 2.6 m up, now with their aim's offsets counted), but 75 degrees
    wide where the demo gave them the robot's 80;
  - the Infiltrator's cars: Jazz `Car_Jazz_CAMSET` (as before, but for its view
    kept within 20 degrees of level hovering and 5.75 m out strafing), Bumblebee,
    Wheeljack and the rest `Car_Bumblebee_CAMSET` (half a metre further back and a
    little higher hovering);
  - the Scientist's jets: `Plane_Starscream_CAMSET`, as far out as before, 75 degrees
    wide hovering (80 before) with the view free within 80 degrees of level (45
    before), and 100 degrees flying (90).
  The DLC's characters the multiplayer set has a camera state of its own for (G1
  Optimus, Slug, Grimlock, Snarl and Vortex) keep that. On foot every class's camera
  is as before. (`tools\class_data.py`'s `vehicle_camera_set` and
  `tools\build_class.py --drive`, which now writes the `vcamera` lines too, with the
  tanks' fields of view; `assets\models\<class>\<class>.txt`; `src\character.rs`,
  `src\main.rs`; the test `the_vehicles_cameras_are_escalations`.)
- **The sword swing lunges at the enemy under the crosshair.** As in Escalation,
  pressing Melee with an enemy in front of you and the crosshair on it now carries
  you at it as the swing begins: a burst at 18.75 m/s for up to a quarter of a
  second (4.7 m) that stops dead as you reach it or your blow lands, after which
  you stand for the rest of the swing. The enemy is the one the game's melee assist
  would pick: within 20 m, ahead of you, in sight, with the crosshair's line within
  its picking radius (3.75 m of a robot's middle, 5.5 m of a jet's, 0.8 m of an
  insect form's), the nearest to the crosshair of any that are. (With no enemy under
  the crosshair it lunges straight ahead: see above.) The game's swing (Escalation's melee set,
  every Escalation character's) flies with `AllowFlight` and an `AttackDash` of
  2500 for 0.25 s at the melee assist's target (`TnMeleeManager.StartFlight`,
  `TnFlier`), stopping on contact (`DisableRootMotionAfterContact`). See *The
  multiplayer melee* (`src\melee.rs`, `src\main.rs`; the tests
  `the_melee_assist_picks_the_enemy_under_the_crosshair`,
  `the_lunge_flies_at_its_enemy_and_stops_on_reaching_it` and
  `a_sword_swing_lunges_at_the_enemy_under_the_crosshair`).
- **One kill, one line in the kill feed, however many players.** Online, a kill
  showed once for each player in the match: three players, three lines of
  "LegoDude [gun] Soldier" for one dead Soldier. Every PC plays each player's
  rounds and blows on its own copy of the enemies (and its own traps), so the
  enemy died on every PC, and each PC put its own line in its feed and sent it to
  the others. Each line now names the enemy that died by its number in the waves,
  and each PC shows one line for each enemy's death: the first it has, its own or
  one sent to it; the rest are dropped (a line it didn't show isn't sent on
  either). The game's server sends one death message for each death; this is the
  same. The players' own deaths were already one line each and still are. The
  network protocol is now version 40, so everyone in a match needs this version
  (`src\killfeed.rs`, `src\net.rs`; the tests `each_enemys_death_is_shown_once`
  and `a_kill_seen_on_three_pcs_is_one_line_on_each`, which plays the video's
  three-player kill).
- **The Snipers' shots look and sound as in the game.** Two things were missing.
  The tracer: the game's (`Tracer_Sniper_AI_FX`) is a bolt flying from the gun
  along the shot (its `Bolt` emitter, itself never drawn) with two strands of
  lightning riding it, which take each of the bolt's particles' place, speed and
  age every frame (UE3's `LocationEmitterDirect` module). The demo's effects
  reader dropped the undrawn bolt and didn't know that module, so the lightning
  was made standing still at the muzzle and never drawn: no tracer at all. The
  bolt is now kept (undrawn) and the lightning follows it, at the shot's speed
  (450 m/s) for its 0.1 s, the effect ending with it as the game's does
  (`bKillOnCompleted`). And the sound: a Sniper's round that passes within 10 m of
  you (or hits you) now flies past with the laser sniper's whoosh
  (`BL_WPN_GUN__IMPT_BYS.BY_SNIPER`, `WHSH_BULLET_BY_LASER_SNIPER_01` to `_04`, at
  the point of its path nearest you), the body of the shot you hear in the game
  after its chirp; the demo played the chirp alone. The same goes for the
  Sharpshooters' shots, which use the same effects. Other effects with the same
  module now follow their particles too: the glow on the ejected shells
  (`Shell4_FX`, `Lrg_Shell1_FX`, `AssaultRifleArm_Shell_FX`) rides each shell, and
  the front and glow of the EMP shotgun's tracer ride its carrier (where the demo
  left them at the barrel). Those whose source is a beam (the Techvolt's extension
  tracer, the heal beam) or comes later in its effect (the Scrapmaker's tracer's
  trail, the Sniper tracer's smoke) are as they were.
  (`src\levelfx.rs`, `src\ai\mod.rs`, `tools\ue3\particles.py`, `tools\levelfx.py`,
  `tools\build_weapon_fx.py`, `tools\build_sounds.py`; `assets\fx\weapons_fx.txt`,
  `assets\sounds\sounds.txt` and the four new sounds; the tests
  `the_snipers_tracer_rides_its_bolt`,
  `location_emitter_direct_follows_its_sources_particles` and
  `the_guns_sounds_are_in_the_bank`.)
- **Optimus Prime and Megatron 10% louder** (since replaced by the game's own
  mixing: see *Optimus Prime's and Megatron's lines turn the battle down* above).
  The announcer's lines in a match (Optimus Prime's on the Autobot maps, Megatron's
  on the Decepticon ones: the game type as the match starts, each wave's countdown,
  "Wave Complete", the last three and last one, and an ally destroyed) were quiet
  beside the characters' own voice lines, and played at 1.1 times the volume the
  game's cues give them (+0.8 dB).
- **Scrappers no longer get caught in the scaffolding over Downfall's start room.**
  The steel trusses off the start hall's roof (`beam1_STAT`: two along its east edge
  and two along its south edge, 22.7 to 30.3 m up) are open lattices, and the demo
  collided with the lattice itself. The game doesn't: its meshes collide by their
  collision hulls unless they say otherwise (`UseSimpleBoxCollision` and
  `UseSimpleLineCollision`, on by default), and this truss's hull is a solid box
  round it. The game's navigation takes the enemies that spawn on the roof along
  the trusses' tops to jumps off their sides, so in the demo they walked the
  lattice, and a jump off a truss's side could land them on its side bracing
  halfway down, where they stayed (a Shotgunner could also catch on a corner of its
  top for a while). Every standing truss of this kind on Downfall (20; the ones the
  purchases bring down are as they were) now collides as the game's does, by its
  box: the enemies walk the top and jump clear of it. In a simulation, with their
  enemy on the ground beside or under the trusses, four Scrappers from the roof's
  spawn points were all down within 19 s and none was caught; before, with him
  under the south truss, one or two of them were caught on its side bracing and
  stayed there, and only one of the four was down after 40 s. As in the game, the
  box also stops shots and sight lines that went through the lattice's gaps.
  (`tools\build_map.py`'s MAPS `hulls`, and `--hulls` to rewrite a built map's
  collision for them; `assets\levels\downfall_col.bin`; the test
  `downfalls_scrappers_cross_the_roof_trusses`.)
- **The Customize screen's hint buttons with a controller.** The buttons along the
  bottom (Toggle Faction and Close at the class select; Unlock/Lock Camera,
  Randomize, Change Form, Cycle Page and Back in the editor) are the PC version's,
  made to be clicked: the controller had buttons for most of them, but nothing on
  the screen said which, and none unlocked the camera, so the right stick never
  turned it. While you play with a controller (one is connected and was used last;
  the keyboard or the mouse switches back) each hint now shows its button's picture
  before its label, the game's own button icons (`UI_GFxButtonIcons_p`'s
  `Gamepad_*`, put before the label as the movie's hint renderer puts them):
  **X** Toggle Faction, **B** Close and Back, the **right stick's click**
  Unlock/Lock Camera (new: unlocked, the right stick turns round the robot), **Y**
  Randomize, the **left stick's click** Change Form, and **D-pad right** Cycle Page
  (the right trigger as well). With the mouse and keyboard the bar is as before.
  (`src\customize.rs`, `src\customize_ui.rs`; `tools\build_customize.py
  --pad-icons` exports the six pictures to `assets\customize\img`; the tests
  `a_controller_has_every_hint_on_its_button`,
  `the_hint_bar_shows_the_buttons_of_whatever_plays` and
  `a_hints_button_picture_and_label_fit_its_button`.)
- **Paste into the text prompts.** Ctrl+V (or Shift+Insert) now pastes the
  clipboard's text into Join Game's address, the Game Password prompts (hosting and
  joining), Display Name and the Customize screen's rename prompt. A paste goes in
  as typing it would: its first line only (a copied address or password often ends
  with a line break; an address's spaces round it are dropped), the characters the
  prompt takes (no spaces in an address), up to the prompt's length, after what is
  already typed; Ctrl+V's V isn't typed as well. The clipboard is read from
  Windows directly (its `OpenClipboard` and `GetClipboardData`), so the build
  needs nothing new (`src\clipboard.rs`, `src\menu.rs`, `src\customize.rs`; the tests
  `a_paste_is_its_first_line_typed_as_far_as_the_field_goes` and
  `ctrl_v_and_shift_insert_paste`).
- **The spectating hint's buttons.** On a PC with the keyboard, the respawn
  screen's "[ Cycle Players ]" is the game's own hint, word for word: its orange
  [ and ] are the keys themselves (the game binds `SwitchSpectatorTarget` to
  `LeftBracket` and `RightBracket`), and they go through the teammates in the
  demo as in the game. With a controller the game shows the LB and RB bumpers'
  pictures at the hint's ends (`GamepadButtonLB` and `GamepadButtonRB`, its
  `setImageSubstitutions`), where the demo had written "LB" and "RB" in orange;
  it now shows the pictures, each 34 pixels square at the ends of the line, the
  words a little lower as the pictures make the line taller
  (`src\hud.rs`; `tools\build_hud.py` exports the two pictures, `assets\hud\img`
  and `hud.txt`; the test `the_spectating_hint_shows_its_keys_and_bumpers`).
- **The Snipers of Ancients and Ignition are purple, as in the game.** The
  Decepticon Snipers had been painted in their character blueprint's colours, red
  with gold trim (`PrimaryColor` and `SecondaryColor`), which the game itself
  doesn't show on them: in the game they wear their own material's paint
  (`RB_SniperDec_MatInst`, and their insect's `VH_SniperDec_MatInst`: lavender,
  sRGB 181 158 233, with gunmetal trim, 112 113 124), as your screenshot of the
  insect on its wall shows. Both forms are now painted that way, the same on both
  maps; Downfall's Autobot Snipers keep their red and white. (`tools\build_enemies.py`'s
  `OWN_PAINT`, which writes their `colours` lines in `enemies.txt` from the
  material; the test `reads_the_enemies`.)
- **Leapers no longer drag the frame rate down.** The Leaper's animations light
  its boosters' glow (`Leaper_Boosters_LowBurn_FX`, an effect that loops for good,
  with heat distortion) on four sockets with each fist attack, smash recovery and
  hit reaction it plays. The game turns those off with the clip that lit them (the
  notify's `bAutoDeactivate`: as the clip ends, or for a looping clip its length
  and 0.04 s after it starts), and keeps one glow on each socket, the one already
  burning playing on rather than another starting. The demo did neither, so every
  attack and every hit left four more glows burning on the Leaper for as long as it
  lived. In a 40 s headless fight on Ignition two Leapers lit 374 of them (over
  a thousand emitters, more every second), and fighting them up close, as in
  your Ignition video, the frame rate fell with each attack and stayed down. Each
  effect an enemy's animation plays now knows whether the game turns it off with
  its clip (the `ai_anims_<set>.txt` files' `fx` lines have a new last field), and
  those go off on time and are never doubled on their socket: at most four glows
  burn on a Leaper at once, and none after it calms down. The other enemies'
  animation effects the game turns off this way (most of their boosters, trails
  and glows, 390 of them) go off on time too, and the ones it leaves on still play
  out. Online, the host sends each one's time with it (the network protocol is
  now version 39: everyone in a game needs this version). (`src\ai\anim.rs`,
  `src\ai\clips.rs`, `src\ai\mod.rs`, `src\ai\sync.rs`, `src\net.rs`,
  `tools\build_ai_anims.py`, which brings files already built up to date with
  `--effects`; the tests `the_leapers_boosters_go_off_with_their_clips`,
  `an_effect_goes_off_with_its_clip_as_it_plays` and
  `effects_turned_off_with_their_clips_go_off_and_dont_pile_up`.)
- **Footsteps in the transformations.** The game's transformation clips carry
  footsteps of their own (`AnimNotify_Footstep`): the robot lands as he comes out
  of his vehicle (`FS_DEFAULT_LAND` or the heavier `FS_DEFAULT_HARD_LAND`; Jazz,
  Bumblebee, Wheeljack, Kickback and Hardshell land twice, and Ironhide steps out
  with two running steps), and eleven of them jump into it (`FS_DEFAULT_JUMP`:
  Swindle, Optimus Prime, Zeta Prime, Ultra Magnus, Hound, Ironhide, Breakdown,
  Demolishor, Air Raid, Blastoff and Kickback). The demo played none of them. Each
  now plays its class's footstep cue at its time in the clip, as the robot's own
  footsteps do, and only while he is on the ground, as the game finds a floor under
  the foot before it plays one (a jet changing back high in the air makes its
  ordinary landing when it reaches the ground instead). The Customize screen's
  garage plays them too. The transformations that have none in the game stay silent
  (Optimus G1's, and Grimlock's, Slug's and Snarl's: Swoop lands)
  (`tools\class_data.py`, `tools\build_class.py`: the class files' new `vtstep`
  lines; `src\character.rs`, `src\main.rs`, `src\customize.rs`; the test
  `the_transformations_play_their_footsteps`).
- **The Titan's Whirlwind spins round and round.** Once the clip had played
  through, the demo let it play on by itself from its start, so every 1.67 s the
  Titan went back to his wind-up and stood with his blade out for most of a second
  while his trails, whooshes and sweeps carried on round him. The clip is now held
  each frame where the ability's clock puts it, as the game's `LoopOffsetTime`
  loops it: through the wind-up once, then round and round the spin from 1.36 s
  (`src\main.rs`; the test `the_whirlwind_clip_stays_in_its_spin` checks it).
- **The Dinobots sound as in the game: landing, jumping, running and standing.**
  Snarl, Slug and Grimlock play the game's own sounds, and none of the tank's.
  Their vehicles' audio components (`HmPlayerVehicleAudioComponent`) give no sound
  for the engine, the boost (only Grimlock's boost starts with one), the jumps or
  the landings, though their `VEH_DINOBOT` sound set lists tank recordings for all
  of them, which the demo was playing.
  - **Landing**: the demo played the tank's metal clang (`VEH_DINOBOT_01_LAND_*`).
    The game's sound is the Dinobot's own stamp, from its landing clip:
    `SNARL_FS_LAND`, `SLUG_FS_LAND` or `GRIMLOCK_FS_LAND` as it touches down, and
    a second stamp 0.6 to 0.8 s later when it lands on the move (Grimlock's with
    his running footfalls too).
  - **Jumping**: their jump clips' leap (`*_FS_DEFAULT_JUMP`; Slug growls as he
    leaps) in place of the tank's jump.
  - **Standing and running**: the tank engine's hum, treads and turret no longer
    play under them. All you hear now is their feet, tails and growls, as in the
    game.
  - **Footstep timing**: their run clips play 1.2 to 1.4 times as fast as recorded,
    and the demo timed the footfalls by the recorded clip. Each run's second
    footfall fell past the end of the faster clip and never played, so the demo
    gave one footstep a stride where the game gives two. Grimlock's walking steps
    were off the same way.

  (`src\car.rs`, `src\character.rs`, `src\main.rs`; `tools\class_data.py` reads
  each vehicle's audio component and times clip sounds at the clip's `RateScale`;
  `tools\build_class.py` writes them, and its new `--sounds` pass brings a class's
  file up to date without rebuilding it. Rebuilt: `models\infiltrator\infiltrator.txt`,
  `models\destroyer\destroyer.txt`, `models\titan\titan.txt`,
  `models\scientist\scientist.txt` (`vaudio` lines only), and `sounds\sounds.txt` with
  their five new cues, whose recordings the demo already had.) The test
  `the_dinobots_play_only_their_own_sounds` checks it.
- **The enemy soldiers' rifles sound as in the game.** Their shot is the Photon
  Eliminator's `SHOOT` cue, the one the Photon Burst Rifle plays, but the game plays
  it differently for them: once for every round, each a little higher or lower
  (the Photon Burst Rifle plays it once a burst), and from a few metres off as its
  distant recording. The cue's layers follow how far from the camera it plays (their
  `SOUND_DISTANCE` volume curves): its close-up burst, the Photon Burst Rifle's own
  sound, only within 5 to 7.5 m, its `MID_DIST_02` shot from there out. The demo
  played the close-up burst once a burst at any distance, so the soldiers sounded
  like your Photon Burst Rifle. Your recording of the game matches the new sound:
  its shots come 0.11 to 0.13 s apart (the soldiers' fire interval), each with the
  distant recording's ring at a pitch of its own. Every cue with such curves now
  follows them (the enemies' shotguns, rocket launchers and grenades, the other
  players' guns; your own gun is heard close up as before, without the distant
  layer the demo used to play quietly under it). And a soldier's round that passes
  within 10 m of you whines past, as in the game (the rifle's `BY_BALLISTIC`
  fly-by, `HmPawn.NotifyNearlyShot`). (`src\ai\mod.rs`, `src\sound.rs`,
  `tools\build_sounds.py`; rebuilt `sounds\sounds.txt`, with 11 new recordings,
  `sounds\WHSH_BLLT_BY_PELLET_AIRY_*.ogg` and `sounds\WHSH_BULLET_WHINE_*.ogg`.) The
  tests `a_guns_shot_far_off_is_its_distant_recording` and
  `a_layer_its_distance_silences_does_not_play` check it.
- **The rechargeable shield fits the smaller classes.** The shield's sphere was
  drawn at the class's skeleton's own size, before the class's scale shrinks it,
  so it stood well clear of an Infiltrator (1 / 0.6 of the sphere round his bones).
  The game's powering effect sits on the character's mesh and takes its size
  (`PlayPoweringEffect` divides by the mesh's `Scale3D`, which stays 1: the class's
  size is its `Scale`), so the sphere is drawn round his bones as he stands, for
  every class: the Infiltrator's is 0.6 of what it was, the Scientist's 0.7, the
  Destroyer's 0.8, the Titan's 0.9, hugging him as the game's own recording shows.
  The ripple a hit makes keeps its size, as in the game (`src\shieldfx.rs`; the test
  `the_robots_sphere_is_drawn_round_his_bones` checks it).
- **A build into another folder runs as it is**: `tools\extract_game.py --assets
  <folder>` now also copies the demo's own shaders into that folder
  (`assets\shaders`' hand-written ones: the build only makes the effects' shaders),
  so the fresh build can stand in for `assets` without a "Path not found" for
  `paint.wgsl`, `hud.wgsl` and the rest.
- **The Titan's Whirlwind looks and sounds as in the game's multiplayer.** It
  played the Infiltrator skeleton's 5.9 s spin (`Shared_SCOUT_ROBO_ANIM`'s) with that
  clip's effects and sounds; the Titan has his own, 1.67 s long (`Titan_ROBO_ANIM`'s
  `Transform_Whirlwind_ROBO`): he draws his Ion Blade and winds up, then spins round
  and round from 1.36 s, as the game loops it. Each turn the clip's two trails sweep
  round him: the white-hot burst on his hand (`Weapon_Trail_Melee_FX`, which bends
  the picture behind it) and the blade's swirl of distortion
  (`Sword_Trail_Whirlwind_FX`), with the clip's `WHIRLWIND_START` and whooshes; the
  camera pulls out to 11.75 m and widens as the game's does while he spins; the
  sweeps come at the clip's times (two in the wind-up, one a turn: 14 with a full
  bar). The other players see and hear it too (`src\whirlwind.rs`, `src\melee.rs`,
  `src\robot_cam.rs`). Rebuilt: `models\robot_extras.glb` and `.txt` (the Titan's
  clip), `fx\weapons_fx.txt`, the effects' shaders and three new textures
  (`textures\levelfx\Energon_Waterfall_*.ktx2`: the trails). The tests `a_full_bar_lands_fourteen_sweeps`,
  `the_spin_goes_round_from_its_loop_offset` and `the_whirlwind_takes_the_camera_out`
  check it.
- **The Dinobots jump and land as in the game.** Snarl, Slug and Grimlock now play
  their own jump clip from a jump until they come down (looping, as the game plays
  their `HoverJumpAnim`) and, after half a second in the air, their landing: on the
  move (or running) the hard landing (`HoverLandAnim`), standing still the idle one
  (`HoverIdleLandAnim`), once through, then back to walking (`src\car.rs`). Their
  vehicle models gain the three clips (`models\infiltrator\vehicle_snarl.glb`,
  `models\destroyer\vehicle_slug.glb`, `models\titan\vehicle_grimlockmultiplayer.glb`;
  `tools\build_class.py`'s `WALKER_CLIPS`).
- **JAMMED goes once the jam is over.** A Guard's scanner jams your ability for 10 s
  and the HUD shows JAMMED pulsing under the ability's icon; when the jam ended the
  icon went back to its colours, but JAMMED stayed for good. The HUD movie sends the
  label's clip to its "off" frame with `gotoAndPlay`, an empty frame with a `stop()`
  on it, and the demo's Flash player only obeyed a `stop()` on a frame it played into,
  not one a `goto` landed on, so the clip played on into "on" and pulsed for ever. A
  `goto` now runs the frame it lands on as Flash does (`src\hud.rs`): JAMMED goes as
  the jam ends. The test `jammed_goes_as_the_jam_ends` checks it.
- **The Gear Shredder's path shows the game's colours**: fine aim's lines along each
  blade's path were black. Its colours (`TnSawBladeProjectileExtension`'s
  `FlyingOnMiss`, `HomingOnMiss`, `FlyingOnHit` and `HomingOnHit` in the blade's data)
  had been read as black; they are blue while the blade would miss and red once it
  would strike an enemy, its brackets the same, as in the game (`src\riot.rs`).
- **Downfall's Snipers no longer cling to one wall for good.** A Sniper coming in
  looks along the air network from its spawn point, a step at a time, for a free
  perch (`FindPathToNearbySniperPoint`), and while Downfall's doors are locked the
  perches beyond them are taken, so from 12 of the map's 20 air spawn points that
  search finds none; it then flies straight to the nearest free perch it can see
  (`FindClosestPoint`). From four of them (`TnSurvivalAirSpawnPoint_10984`, `_1264`,
  `_854` and `_8571`) that was `TnSniperPoint_9893`, which in the game's own map has
  no flights to or from it at all (no `NavConnections` either way: every other
  perch on the map has some). Once there a Sniper could never find a perch to move
  on to; it clung to that wall for the rest of the wave, firing only when a player
  came into its view. The nearest perch it can see is now one it can fly on from, and
  that it has room to fly to straight, measured as the match measures the air
  network's flights (the next nearest, `TnSniperPoint_14386`, is in sight from those
  four but an overhang the line passes under stops a Sniper's body; flown at, it hung
  there) (`src\ai\sniper.rs`). In a simulated run of a Sniper from each of
  Downfall's air spawn points (two and a half minutes each, the player moving about
  the map, twice over), the Snipers from those four perched once and stayed; now
  every one perches 15 to 24 times, and Ancients' and Ignition's are unchanged. The
  test `downfalls_snipers_go_on_from_their_first_perch` checks it.
- **Ancients' Snipers no longer hang in the scaffolding.** From its perch a Sniper
  often flies straight to a perch beside it (one its navigation links to:
  `GetSurroundingPoints`), and on Ancients a few of those flights pass through the
  lattice of scaffolding round the map's middle, about 25 m up (both ways between
  `TnSniperPoint_10859` and `TnSniperPoint_7942`, and from `TnSniperPoint_14899` to
  `TnSniperPoint_2368` and `TnSniperPoint_16349`: less than a metre of room where a
  Sniper needs 1.75 m). The match already measures each flight's room against the
  map as it starts and keeps a Sniper off those too narrow for it on its paths, but
  this straight hop to a neighbour didn't ask: the Sniper flew into the girders and
  hung there for the rest of the wave, each push off them taking it back in (the
  Sniper in your video). Now it passes over a neighbour it hasn't the room to fly to
  and picks another (`src\ai\sniper.rs`). In a simulated run of a Sniper from each of
  Ancients' 16 air spawn points (two and a half minutes each, the player moving about
  the map, twice over), seven of the 16 in one run and three in the other perched
  only once and hung in the air the rest of the time, and most of the others hung
  for a while; now none hangs and each perches 15 to 24 times. Ignition's
  and Downfall's Snipers are unchanged. The test
  `ancients_snipers_never_hang_in_the_air` checks it.
- **The lobby menu's text sat too high**, its tops at the selection bar's top edge:
  its labels and values were placed without the 2 pixels a Flash text field keeps
  between its edge and its text (the main, settings and pause menus had them). They
  now sit 2 pixels lower, inside the bar, as the game has them; the Extras menu's
  label, placed the same way, now sits in the middle of its bar too
  (`src\menu.rs`).
- **A jet's sticky rocket no longer leaves its glow hanging in the air**: changing
  form to shake one off plays its detach effect where it was
  (`FlyersSticky_Detach_FX`), whose data keeps the rocket's crackling electric ball
  going for eight loops, 24 s; it hung there long after the rocket was gone. It is
  now let go after a second, once its flash and sparks have played. (And the glow a
  jet's robot holds before its pulse attack goes with the robot if the jet changes
  back into its plane mid-attack.)

**Earlier** (graphics settings, shadows and performance)

- **"boot demo cargo.bat" now builds and plays the optimized (release) build**,
  which runs big waves much more smoothly. Its **first build takes about eight
  minutes** (the engine is built once, fully optimized); after an update the
  demo's own code rebuilds in about two minutes. The old way, the quicker-to-build
  but slower-to-run debug build, is still there as **"boot demo cargo - dev
  build.bat"** beside it. (`Cargo.toml` has the new `[profile.release]`.)
- **Faster big waves.** Measured on your PC in an Oblivion wave 15 (14 enemies
  fighting in view, release build): the frames went from about 18 ms (p95 23 ms,
  spikes to 35 to 55 ms) to about 13 ms (p95 about 15 ms, with an occasional
  spike to about 30 ms), comfortably over 60 frames a second. The biggest cost was the
  sounds: each new sound opened its Ogg Vorbis file again, 3 to 5 ms a frame in a
  fight; recordings are now decoded into memory once (`src\sound.rs`). Effects out
  of the camera's view are no longer rewritten every frame, and in a busy fight
  the effects further off than the nearest 100 move every other frame
  (`src\levelfx.rs`; Extra moves them all every frame); the HUD no longer
  renumbers its draw order when a damage arrow or hit marker comes and goes
  (`src\hud.rs`). At **Low** (below) the same wave runs at about 10 ms.
- **Graphics settings** (SETTINGS > GRAPHICS), with the game's rows:
  - **Graphics: Low / Medium / High / Extra**, the game's own levels, each setting
    the demo's graphics as a group: the sun's shadow map and how far its shadows
    reach, bloom, depth of field, motion blur, anti-aliasing (none, FXAA or 4x
    MSAA), the lights shots and blasts make, the effects' detail and how far away
    they are drawn, the characters' shadows, and (as in the game, from the next
    start: it says "Game Restart Required") texture detail and filtering. **High**
    is the demo's look until now (but for the far effects in a busy fight, above)
    and the default; **Extra** adds sharper texture filtering, the sun's shadows
    further out, more characters' shadows and every effect moved every frame.
  - **Resolution**, the game's slider, with the monitor's resolutions and
    **Desktop** (the default: borderless at the desktop's resolution, as before).
    With Fullscreen Mode on, a resolution is true (exclusive) fullscreen at it; in a
    window, it sets the window's size.
  - **VSync**, the game's (Off until you set it, as before).
  - As in the game, changes to Fullscreen Mode, Resolution and VSync wait for
    **Accept** (beside Back, or Enter); Back leaves them as they were.
  - **Render Scale** (the demo's own), 50 to 100%: the game is drawn that much
    smaller and spread over the window, while the HUD and menus stay sharp.
- **Characters cast shadows from the lights round them**, not only from the sun, as
  the game's light environments do: each character near the camera (players and
  enemies) casts one soft shadow of its own shape away from the strongest light
  near it (`src\shadows.rs`).
- **The Cloak as the game draws it.** Going on, the game's overlay spreads over Jazz
  from his right elbow in a ragged blue front, his body black with an orange rim
  behind it, burning out to nothing as he goes into the cloak, with the game's
  lightning round him; cloaked, nothing of him shows but the picture behind him
  bent by his shape (the game's distortion); like the game's Escalation, it gives no
  warning as the meter runs low (the campaign's waves and warning sound are left
  out); and he comes back at once as it ends, with the lightning again
  (`src\cloak.rs`, `assets\shaders\paint.wgsl`: see "The class abilities" below).
  (The network protocol is now version 38.)
- **The ammo bar as the game draws it.** Its segments' ends no longer stretch with
  them: a weapon with a one-round clip (the jet's Thermo Burst Rockets, the tank's
  cannon, the Thermo Rocket Cannon, the Chaos-Rift Combustor) fills the whole bar
  when loaded, as in the game, and the other weapons' segments look as the game's
  do (`src\hud.rs`, `assets\hud\hud.txt`: see "The HUD" below).
- **Downfall: no more flying out of the start room onto the enemies' spawn roof.** A
  jet could climb the start room's light well and land on the roof where enemies
  spawn; an invisible wall for the players only (the enemies pass it, as they pass
  the game's spawn walls) now closes the light well just under the roof and keeps
  everyone off the roof (`assets\levels\downfall_col.bin`, see "The Downfall map").
- **Melee knocks enemies back, as in Escalation.** The demo's blows were the
  multiplayer melee set's (135 damage, a push that barely moved a soldier); every
  Escalation character uses the game's shared melee set instead, whose sword swing
  does 75 and throws a soldier off its feet (its push 75000 forward and up: in the
  air for over half a second, the game's knockback, with its take-off, flight and
  landing animations), and whose sprinting punch does 150 with the same push. Killed
  enemies are thrown further for it (see "Melee" below).
- **A blow that lands shakes the striker's view and splashes energon across his
  screen**, as the game's melee damage does (`Screen_EnergonBloodsplatter_FX`, the
  drips and splats bending the picture behind them). As in the game, a distorting
  material draws nothing where its opacity is under a 255th (its compiled shaders
  clip there): the splash's splats, whose opacity dips below nothing, had multiplied
  the picture to black in blotches there.
- **Enemies dissolve in the game's cyan energon**, not their faction's colours: the
  game's `TR_DefragOverlay` (a cyan glow eating through the body from where it fell,
  with the circuitry lines and noise of the game's material), over the three
  seconds the game takes (see "The enemies' deaths" below).
- **Distortion drawn once per place.** The effects that bend the picture behind them
  (heat haze, shock rings, the Riot Cannon's blast, the screen's energon) each moved
  the scene by their own shift and added the difference, which piles up where many
  overlap; now each place is moved about as far as the furthest layer over it, as
  the game's single move by the (clamped) sum of their shifts does, and the shift
  is the game's size (a 255th of the screen for each unit, where the demo had taken
  a pixel) (see "The maps' effects").
- **Downfall's Snipers no longer hang in the air.** Leaving their first perch took
  their acceleration away (the demo gave them a walking motion controller's
  settings, none, where the game has no motion controller for them), so they flew
  on unsteered and crawled through the air wherever they touched a wall; and they
  chose perches beyond the locked doors (the points the map's Kismet blocks until
  the door is bought, which the game counts as taken) and flew into the doors. They
  now fly from perch to perch and shoot from there, as in the game.
- **Downfall's Defensive Sentries wake up and fight.** They looked for enemies from
  their heads, which sit up inside the ceiling while the turrets are folded, so they
  saw only the floor straight below them and rarely unfolded after a purchase. They
  now look from their muzzles (the game's `GetViewLocation`), seeing the hangar
  around them: a bought turret unfolds at the first enemy in view and fires. They
  also start folded up instead of unfolding at the start of the match and folding
  as the round began, and their textures are the full-size set (1024 pixels, from
  `WEP_SentryBlaster01_p.xxx`) instead of the map's 128-pixel copy, so the folded
  turret's casing shows its panels.
- **The vehicles' insides are textured as they change form.** The undersides of
  their panels, which show as they fold, were drawn as dark glass on 19 vehicles:
  eleven vehicles' insides are an instance of the game's `Interior_DIF__TEXSET` named
  `VH_Interior_MAT`, a name Swoop's own material shares, and the build took Swoop's
  (no texture set) for all of them; the rest had insides the build couldn't read
  (pictures and plain materials). Each now has its own, from the game's packages:
  Demolishor, Scattershot, Warpath, Ironhide, Ratchet, Slug, Zeta Prime, Bumblebee,
  Jetfire, Silverbolt and Starscream the textured machinery; Breakdown and Sideswipe
  the same from its pictures; Soundwave, Onslaught and Deadend their energon-lit
  insides, glowing in their energon's colour; Snarl and Swoop the Swarmer's; Vortex
  its own (`tools\build_class.py --interiors`, below).
- **Dashing, jumping and landing show the game's effects under the robot's feet.**
  The boosters at his ankles and the soles of his feet fire in his energon colour as
  he dashes or jumps, a dash to the side fires them again to stop, and a landing
  throws up dust, each class as its own clips have it (*The robots' dash, jump and
  landing effects*, below).
- **Downfall's scaffolds come down when their console is bought, so the enemies no
  longer run into them.** Downfall's steel scaffolding (19 pieces, the game's
  `TnStaticDestructibleActor`s, four of them by heavy ammo station 5) is
  knocked down in the game by the purchases that open those ways (one pair of
  consoles brings down 15, another pair 12, a fifth console both sets), and the same
  purchases open the enemies' navigation under them. The demo left the scaffolding
  standing, part of the map, so once a console was bought the enemies were sent along
  ways that ran straight into it and stood pressing against it. It is now its own
  piece of the map's doors: each scaffold goes with the purchase, with the game's
  crash (`DEST_ASH_COLUMN_EXPL`), and takes its collision with it. It **crumbles as
  in the game**: the standing scaffold gives way to its broken pieces, the beam's four
  parts thrown out in every direction (twelve to a scaffold, at 10 to 80 m/s, lit by
  the light where they stood), with a burst of smaller metal chunks, falling,
  bouncing off the level and coming to rest lying flat. As in the game, at most 50
  pieces are out at once, the oldest going first, so of a whole courtyard coming down
  at once the last pieces thrown are the ones left lying for their ten seconds.
- **The Dinobots land on a burst of dust on the floor.** Grimlock's landing dust in
  vehicle form came out of his chest: his landing effect hangs off his lock-on
  socket there, in the game's data as in the demo (his blueprint's landing list is
  the tanks', whose sockets sit low on their hulls). A Dinobot's landing (Grimlock's,
  Slug's and Snarl's) now plays on the floor under its socket (see *Vehicle effects
  and heat haze*, below).
- **The A-4 Pulsar Cannon's ball of plasma sits on the gun's front.** In the right
  hand (where the gun is held unless you switch hands) it sat back by the elbow:
  the demo turned the glow round by the scale of the gun's socket, which the game
  doesn't (see *Firing effects*, below: a socket's scale is no part of an effect
  on it), so its 50-unit offset pointed back along the arm instead of down the
  barrel.
- **The Energon Harvester's arcs go with it when you swap guns.** The arcs round its
  muzzle while it is ready live a second each; when the gun was put away they were
  left hanging in the air where it had been. What is out of a gun's idle effect when
  it stops (firing, reloading, a swap) now rides the gun while it shows, as the
  game's does on the weapon mesh it is attached to, and is gone with the gun as it
  is put away (`src\orbital.rs`); the same for the E.D.K. TechVolt's arcs, the
  Orbital Pistol's steam, the Corrosive Slime Cannon's smoke and the A-4 Pulsar
  Cannon's glow.
- **The Eclipse Rockets fire one burst of two for a click.** A truck's trigger held
  into its first rocket's interval (an ordinary click lasts that long) was counted as
  a second pull, so a click fired a second burst 0.7 s later: four rockets. A held
  trigger now fires another burst only if it is still held when that burst is due
  (`src\main.rs`'s `rocket_burst_starts`).
- **The sun lights Downfall, Ancients and Ignition again.** Their suns lit nothing:
  the layers added over each sky dome (its haze, Downfall's sun glow) surround the
  map just as the dome does, and Bevy's shadow pass drew them as solid, so each
  sun's shadow started at the sky and covered the whole map. As in the game, where
  translucent materials cast no shadow, nothing see-through casts one now
  (`src\lighting.rs`'s `map_material_flags`): Downfall's streets catch the pink
  sunset with the towers' shadows across them, and Ancients' canyon, wrecks and sky
  dome are sunlit.
- **The suns' light shafts, as the game draws them.** Looking toward Downfall's
  sunset, the towers and gantries go dark against it with the rays streaming
  between them; Ancients' sun washes the rocks and scaffolds round it in a warm
  glare; Ignition's gives a fainter glow. The game's own three shaders, read from its
  shader cache, with each map's own light-shaft settings (`src\lightshafts.rs`,
  `assets\shaders\lightshafts.wgsl`; see *The sun's light shafts*, below). They come
  with the bloom, so the Low graphics level draws none.
- **Ancients' sky is the game's bright, warm haze.** Its grey dome was dark (in the
  sun's shadow, above) and its haze dimmer than the game's: UE3's height fog adds
  1 / ln 2 (1.44 times) of its light over what it hides, which the dome's baked haze
  (`assets\models\ancients.glb`) now has. The haze over Downfall's and Ancients'
  streets is that much brighter too, as in the game (their lighting files).
- **No more dark patches in Downfall's sky.** Moving about, small dark shapes
  flickered in the sky: the sunset haze and the sun's glow are drawn on the dome's
  own triangles, just in front of it, and here and there they lost the depth test to
  the dome, letting the bare dome show through. Each sky dome is now drawn a
  fraction further out (0.2%, `src\lighting.rs`'s `SKY_BASE_PUSH`), so the layers
  over it always show.
- **Scrappers no longer pace about below a player on a ledge.** On Downfall they
  gathered in the trench under the Photon Burst Rifle's walkway with a player up on
  it, stepping side to side there for good: the game's Scrappers attack only along
  a straight walk to their enemy, and there is none from the trench (the way up is
  a jump at its end). Now, after two side steps that leave the straight way cut
  off, a Scrapper follows the path up to him, jump and all, and fights him on the
  walkway: in a simulated run, three Scrappers started in the trench were all up
  within about 6 s. This is the demo's own rule, not the game's (*The enemies' AI*,
  below); the other enemy types are as they were (those that shoot hold cover in the
  trench and fire up at the walkway, as the game's do).
- **The weapons for sale look as the game's, on every map.** Each gun on a weapon
  station (and in an Armory Recreator's pod) is now the game's pickup: its
  `TnWeaponMesh`'s `PickupMesh`, the gun's mesh as it is modelled (its reference
  pose), with the **forearm cuff** it is mounted on, the angled piece at its back
  that was missing (the demo had left the mesh's second section out, and posed the
  gun with Jazz's arm, which stood a thin rod up where the cuff goes). It is **shaded
  as the gun in hand is**, not washed out: the game draws it with a
  `PICKUP_*_MATINST`, an instance of the characters' material, painted a neutral grey
  (0.396) with the metallic setting at 0.4, so here it goes through the same paint
  shader, with each pickup material's own gloss values (`src\character.rs`'s
  `paint_pickups`); the Neutron Assault Rifle's textures are now full-size (its
  pickup had 128-pixel ones). **Its size was already the game's** (its `PickupScale`
  is 1 for every weapon): fitting the camera of your two screenshots to the stand's
  lights put the game's Scatter Blaster and the demo's at the same size over the
  same stand, the demo's screenshot taken 1.6 m nearer. **The stands** (and the health
  and equipment stations' bases) are drawn with the game's own unlit material
  (`Pickup_Unlit_MAT`, `src\pickupbase.rs`): their own rust brown, lit by a light
  circling them once every four seconds, where the demo had lit them with the map's
  light maps, a flat grey.
- **The pickups' outlines go round their silhouettes only**, as the game's do: the
  glowing line had also run along the edges of a gun's parts in front of its other
  parts (its barrel over its body, its body over the cuff), lines across the gun the
  game hasn't. The outline is still a shell round the pickup, but it is now drawn at
  the back of the pickup's bounding box as the camera sees it, along each line of
  sight (the same place on the screen, the same width), so the pickup hides it
  wherever it covers it (`src\outline.rs`, `assets\shaders\pickup_outline.wgsl`).
  The weapons on the stations and in the pods, the ammo crates and the support items
  all have it.
- **What an Armory Recreator makes has its outline again**, cyan or yellow by the
  kind of weapon, as on the stations. A weapon in a pod has its back (its forearm
  cuff) in the pod's back wall, so the outline, moved back behind the weapon, went
  behind the wall too and was hidden. In a pod it now goes no further back than just
  in front of the pod's back wall (`OutlineBackstop`, `src\recreator.rs`'s
  `back_wall`); the stations' outlines are as they were.
- **The Chaos-Rift Combustor's shot flashes once**, as in the game: an orange-white
  flash at the muzzle for a sixth of a second and some dark smoke, where the demo had
  a cream-white cloud hanging over the gun for a second after every shot. The flash's
  cap and core are each one burst of one; an emitter that sets no spawn rate of its
  own has the engine's default, which the game's files give as 0 (Engine's
  `Default__ParticleModuleRequired`), but the effects had been read with 10 a second,
  so the cap flashed on through its emitter's whole second. Read right
  (`tools\ue3\particles.py`), 350 emitters' rates in `assets\fx\weapons_fx.txt`
  are now 0: other weapons' and enemies' muzzle flashes, blasts and sparks spawn only
  what the game's do, so some of them are a little shorter or thinner than before
  (the Riot Cannon's blast loses two of its smokes, and the tank shell's blast a dark
  fire burst, which have no bursts and so never spawn in the game).
- **No more giant grey picture over the screen when the Combustor's clusters go
  off.** As a cluster went off, its canister's mesh was taken away in the frame its
  blast's effects came, and for that frame Bevy drew the canister's metal texture
  over the blast's huge sprites, filling much of the screen. An effect's pieces that
  are done are now hidden first and taken away a few frames later
  (`src\levelfx.rs`'s `Retired`), which keeps them out of the frame's drawing.
- **No more lilac orbs hanging in the air after a Scrapper's attack.** The
  Scrappers' fist attack is the Leaper's animation, which fires the Leaper's
  boosters and arm trails on sockets a Scrapper's mesh hasn't. The game plays an
  effect only on a socket the mesh has (it can't attach one to a socket that isn't
  there, and nothing shows); the demo had played them at the Scrapper's middle
  instead, where the boosters' endless glow stayed, an orb for every attack. An
  animation's effect on a socket its model hasn't now isn't played
  (`src\ai\mod.rs`'s `effect_frame`).
- **The shield's recharge sphere takes in the whole robot**, as in the game. The
  game measures the sphere round the robot's bones in his skeleton's own space,
  before his class's size (its `RobotMeshMergeScale`: the Infiltrator 0.6, the
  Destroyer 0.8, the Titan 0.9, the Scientist 0.7), so it is that much bigger than
  the robot as he is drawn: 1.1 to 1.7 times the sphere round his bones as he
  stands. The demo had measured it round his bones as drawn, too small to cover his
  shoulders and arms. It is also measured the first time an effect needs it, as the
  game's is (the demo had measured it as his robot came in, before he was posed),
  and the hit ripples lie on the same sphere (`src\shieldfx.rs`).
- **Snipers no longer hang in the air on Ignition** (or drift into its walls and
  ceilings). A Sniper that caught sight of a player just as its flight to a perch
  ended had its plan checked against what it now saw before it took to the wall,
  where the game checks it only once the Sniper is on the wall (the game skips that
  check while the operator about to start can't be interrupted, and taking to the
  wall can't). Its plan dropped in mid-air, in its robot form, it had nothing left
  to do there and hovered on the spot for the rest of the wave, slowly drifting
  against whatever was near (`src\ai\brain.rs`). It now takes to the wall and
  fights from it, as in the game. In a simulated run of a Sniper from each of
  Ignition's eight air spawn points (two and a half minutes each, the player moving
  about the map), four had perched only one to three times, stuck in the air for
  the rest; now each perches about 20 times and none hangs (and none in five-minute
  runs either). The test `ignitions_snipers_never_hang_in_the_air` checks it.
- **The Destroyer and the Titan swing without the Ion Blade.** Only the Infiltrator's
  and the Scientist's sword swings bring the blade out of the gun (their clips carry
  the game's `TnAnimNotify_WeaponMelee`); the demo gave the Destroyer's and the
  Titan's the blade too. Now they swing without it, as in the game
  (`models\weapons\melee.txt`, `src\melee.rs`, `tools\build_melee.py`).
- **Ignition's Snipers fly through the AirStrike station's barrier.** The invisible
  barrier that keeps players off the gunship console's platform until it is bought
  (`DynamicBlockingVolume_16006`) also holds the hub of nine of the Snipers' flight
  paths, which the game leaves open; the demo stopped the Snipers at it as if it were
  a wall, so those flying through hung against it. A Sniper in flight is now stopped
  only by the level's solid surfaces (this barrier is the only one that crosses the
  flight paths on any map).
- **A teammate who self-destructs while downed no longer flickers on the HUD.** On
  the other players' screens his health bar and marker blinked on and off until the
  next wave: between the host's updates, his copy's wait ran out and he counted as
  alive for a frame. His copy now stays out until his own PC says he is back.
- **The dust from dashing, jumping and landing is lighter, as in the game.** Each
  burst kicked up up to 90 puffs of dust and as many sheets on the ground, a thick
  beige fog round the robot for half a second; it now keeps to the game's own limits
  (20, 3 or 3 puffs at a time and one sheet), the thin ring of dust about his feet
  that the game's videos show. A car's short landing (the same effect) is lighter too.
- **Oblivion's exploding barrel**, as the game has it by the start: the Autobot
  energon barrel (`TnStaticDestructibleActor_4264`, its blueprint
  `DES_AutobotExplodingBarrel_DSYS`) stands where the map puts it, glowing red with
  the red outline round it, and turns the crosshair red as an enemy does. It takes
  25 damage (an enemy's rounds count half, a blast a quarter unless it goes off
  against it) from the players' guns, rounds and blows and the enemies' rounds,
  rockets and grenades, then blows up with the game's explosion and sound: 150
  damage to everything within 15 m in sight of it, flinging its pieces (one big
  piece trailing smoke and sparks, six burning chunks), leaving its wreck burning
  for 10 s and then standing charred for the rest of the match. Blown up by a
  player, it hurts the enemies (Escalation's players never hurt each other); by an
  enemy, the players. Each match starts with it standing (*Oblivion's exploding
  barrel*, below).
- **The gun goes away while sprinting, as in the game.** As a robot breaks into a
  sprint his gun folds away into his forearm over a second (the weapon's holster
  clip and sound, the game's `DetachDelay`) and his bare forearm unfolds in its
  place, and as he slows to a jog it comes back out (its equip clip and sound) while
  the forearm folds away, with the shoulder pads moving as for a weapon change; the
  same while he gives a revive. Firing or fine aiming ends the sprint, and the gun
  fires once it is back in his hand; he can't switch hands while it is away
  (*Putting the gun away to sprint*, below).
- **Quicker to start, and the Customize screen opens at once, whole.** Every effect
  (the maps', the garage's and the weapons': 308 materials by now) was drawn with
  the whole of `assets\shaders\levelfx.wgsl`, some 650 KB, and Bevy works through
  the whole file for each effect pipeline it makes, one pipeline at a time: about
  60 ms each on your PC. With every update adding the weapons' effects it had grown,
  until the 250 to 300 effect pipelines the demo makes as it starts took about 16 s,
  everything else the game asked for waiting behind them. Each effect material now
  has a shader of its own, made as the demo starts (the code they share and its own
  function, some 18 KB: about 6 ms each), and the same pipelines are all made
  within about 1.5 s. On your PC, opening Customize 3 s after the menu came up took
  9 to 14 s (the game stood still while Bevy made a pipeline the screen's cameras
  needed, behind all the others) and then showed an untextured garage and black
  robots for a second as the rest came; it now opens in about half a second, whole.
  The screen also stays black until the garage and the four robots can be drawn
  whole (everything loaded, the robots dressed and no pipeline left in the making),
  then fades in; and the pipelines that put a camera's picture on the screen are
  made as the demo starts, so opening it early can't hold the game up either. The
  "textField" text and glows you saw behind the screen as it opened were the HUD,
  still drawn under the menus: it is only drawn in a match now (*The maps'
  effects* and *The Customize screen*, below).

**What's in the demo**

- **The game's main menu** first, rebuilt from Fall of Cybertron's own front end: the burning-Cybertron background movie, the logo, CUSTOMIZE / ESCALATION / SETTINGS / EXTRAS / EXIT GAME with the game's selection brackets, colours, fonts, descriptions and animations, its button sounds and its menu music. **ESCALATION** opens the game's Escalation lobby (below), **SETTINGS** opens the game's PC settings menu: Display, Graphics, Audio and Controls, with **Graphics** holding the game's Graphics level (Low/Medium/High/Extra), Fullscreen Mode (On/Off), Resolution and VSync, and the demo's Render Scale and Framerate Cap (30/60/90/120/None) (see *Graphics settings and performance* below), **Audio** a Master Volume slider over the game's own FX Volume, Dialog Volume and Music Volume sliders, and **Controls** the game's controls settings and keyboard layouts (below) (all remembered between runs), and **Display** the game's Brightness and Safe Frame screens and its Team Coloring Mode (below), **EXTRAS** opens the game's Extras menu with **Credits**, which plays the game's own ending credits movie with its music (skip it with Esc, Enter, a click or a gamepad button), **EXIT GAME** asks the game's question (Warning: Do you want to exit the game? Continue or Cancel) and quits, and **CUSTOMIZE** (the game's multiplayer item, named with the game's own lobby text) opens the Customize screen (below)
- **The game's Customize screen** (MAIN MENU > CUSTOMIZE), rebuilt from the multiplayer character customization (`UI_GFxCustomCharacter_p.CustomCharacter_GFX`) and its garage level (`UI_CustomCharacter_m`): the garage itself with its baked lighting, lights, light beams, lens flares, steam and the glow over the chosen class, its ambience, and four preview robots on its platforms in the game's customization pose. All four classes' preview robots are their own characters, each at its class's size (the Scientist's a little taller than the Infiltrator's, the Destroyer's a head taller, the Titan's taller still). The **class select** flies in under them as the game's does: the four class tags with each class's level, its experience bar with the marker and what the next level still needs (MAX at the top) and three build slots each (the second and third open at the class's levels 5 and 10: until then they read "Unlocks at Level 5" and can't be opened; see *Experience and class levels* below), the focused tag expanding as its slots fold out, with the game's camera moves between the line-up and each class. Choosing a slot opens the **editor**: the class tag flies to the corner with the build's name, and the game's turned 3D stack of pages swings in: **Loadouts** (primary and secondary weapon, their upgrades, ability and ability upgrade, with the weapon and ability info panel: description, upgrade, wireframe picture and the damage, range, accuracy and rate-of-fire bars), **Armor** (head, chest/vehicle, shoulders, arms, legs, wheels (the Titan's cannon, the Scientist's wings), decals and armor set, with the class's Analysis panel) and **Personality** (Autobot and Decepticon colours with the game's colour grid, the Metallic slider, voice and Rename Build). Lists and the colour grid open over the page with the game's rows popping out toward the camera, locks on the weapons, upgrades, abilities and ability upgrades the class's level hasn't opened (with the level each needs) and "In Use" on the weapon the other slot has, and the camera's close-ups of the part being chosen. The hint bar has the PC version's buttons: Unlock/Lock Camera (then drag or Q/E to turn round the robot), Randomize (as the game's: a random part of each kind you can wear, random colours for both factions and a random metallic finish; the loadout and voice stay), **Change Form** (the preview folds into its vehicle with that vehicle's own transformation, at the game's own speed as in a match, settling onto its wheels (a Titan's tank or a Scientist's jet rising to hover), and back, landing in the customization pose; pressed again mid-change, it changes back as soon as it is done), Cycle Page and Back. The game's data fills the lists: every class's weapons, upgrades, abilities, voices and colour swatches (108 for each side) from its ini files, with their names and descriptions. Builds are saved in `builds.txt` next to the `assets` folder. Every class offers its own parts and armor sets (below), the game's free ones open and the rest sold for the shards a level brings, at the game's prices (see *The Customize screen's shop* below), and the preview robots wear the build's parts and colours (Toggle Faction shows its Decepticon colours and purple glow). **Select Voice** offers the class's three voices and **None**: the voice is the character's in a match (see *The characters' voices* below), the preview robot says a line in it as the list moves to it, and None keeps him silent. In every class's Loadouts the preview robot holds the weapon being looked at (any of the game's sixteen: the Infiltrator's Scatter Blaster, Path Blaster, Subsonic Repeater and Nucleon Charge Rifle, the Destroyer's Riot Cannon, A-4 Pulsar Cannon, Thermo Rocket Cannon and Chaos-Rift Combustor, the Titan's X18 Scrapmaker, Energon Harvester, E.D.K. TechVolt and Corrosive Slime Cannon, the Scientist's Neutron Assault Rifle, Photon Burst Rifle, Orbital Pistol and Gear Shredder), standing the way the game's preview character stands with a gun (each class its own way), as the game hands it the weapon there and then. The builds are the characters you play as in a match (Choose Character, below)
- **The Infiltrator's characters**: all eight of the game's Infiltrator armor sets, Bumblebee, Breakdown, Sideswipe, Deadend, Jazz, Dragstrip and the DLC's Wheeljack and Snarl, with the bonus heads and extra wheels, each chest's own vehicle (Snarl's a walking dinosaur), its transformation, and the parts' own animations (the forearms folding away as the gun changes hands, the shoulder pads flipping as a gun comes out), painted with the build's colours as the game paints them. Customize mixes and matches the parts as the game does, and in a match Jazz is the chosen build, as the other players see him too (The Infiltrator's characters, below)
- **The Destroyer's characters**: all eleven of the game's Destroyer armor sets, Swindle, Optimus Prime, Ratchet, Ironhide, Soundwave, Onslaught and the DLC's Zeta Prime, G1 Optimus, Ultra Magnus, Hound and Slug, with their bonus heads and wheels, each chest's own truck (Swindle's and Hound's jeeps, Slug a walking Dinobot) and transformation, painted the same way. A Destroyer is the class's own size (bigger and broader than an Infiltrator: his collision, the camera and where shots come from all follow), moves with the class's own heavier jog, sprint, dashes, jump and fine-aim walk and swings his own sword, with heavy footsteps; his camera is the class's own, further out; and his trucks drive as the game's trucks do (slower and heavier than the cars, with their own jumps and hover), with their own engines, tyres, crashes and death, and fire the game's **truck rockets**, the Eclipse Rockets, from launchers that rise out of them as they fire: with the multiplayer's clip, reserve and fire rate (8 to a clip, 24 in reserve, bursts of two 0.15 s apart, one burst for each click, and while the trigger is held another 0.7 s after each burst's second rocket, a 2 s reload), rockets that fly straight and blow up (55 damage within 15 m), with their sounds and effects (The Destroyer's characters, below)
- **The Titan's characters**: all nine of the game's Titan armor sets, Megatron, Warpath, Brawl, Scattershot, Demolisher, Quake and the DLC's Perceptor, Hardshell and Grimlock, with their bonus heads and cannons (a Titan's sixth part, in the wheels' place), each chest's own tank (Grimlock a walking T-Rex) and transformation, painted the same way. A Titan is the class's own size, the biggest yet, moves with the heavy moves and swings his own sword, with heavy footsteps, and has the class's own camera; his tanks handle as the game's do: they only hover, the hull turned to where you look, strafing in any direction or boosting straight ahead, on the game's hover jets and boosters, and they fire the game's **tank cannon**, the Mega-Concussion Cannon, which aims with the view and fires a shell at a time (the multiplayer's: one loaded and 9 in reserve, the next loaded in 2.25 s) that flies and blows up (300 damage within 12.5 m), with the cannon's recoil, reload, sounds and effects, the tanks' own engines, jumps, landings and transformations (The Titan's characters, below)
- **The Scientist's characters**: all ten of the game's Scientist armor sets, Jetfire, Starscream, Air Raid, Vortex, Silverbolt, Shockwave and the DLC's Blastoff, Sharpshot, Kickback and Swoop, with their bonus heads and wings (a Scientist's sixth part, in the wheels' place), each chest's own jet (Vortex's a helicopter, Sharpshot's and Kickback's Insecticons) and transformation, painted the same way. A Scientist is the class's own size (a little bigger than an Infiltrator), moves with the Infiltrator's light moves and swings his own sword, with the medium robots' footsteps, and has the class's own camera; his jets handle as the game's do: they answer the keys from the moment the change into them starts (flying at once if you hold boost as you change), **hovering**, the body turned to where you look, strafing in any direction (up and down with the view too), climbing with Space and diving with Control, and **fly** while you boost, thrusting along the view at up to 40 m/s, banking into turns, with the game's flying camera, which turns the way the mouse asks at the game's own rates and rolls into the turns; they drop back to hovering when you let go or crash. They burn the game's hover jets, afterburners and vapour trails, sound their own engines (each jet's hover, whine, boosters, boost loop, climbs and dives), crashes and transformations, and fire the game's **homing rockets**, the Thermo Burst Rockets, as in multiplayer: a rocket for each pull of the trigger (a clip of one, 11 in reserve, the next loaded in 1.5 s) that locks on to an enemy kept under the crosshair for a quarter of a second (held for 3 s after it leaves it) and homes in on it (75 damage within 5 m), with their sounds and effects (The Scientist's characters, below)
- **The characters' glowing accents, in their faction's colour:** every armor set's robot and vehicle glows where the game's materials light it (each texture set's glow mask, the red of its `_Masks_NormY`). Its energon accents, Jazz's eyes, wheel rims and leg stripes, Deadend's chevrons, Breakdown's wheel spokes, glow in the faction's energon colour as the game's `TR_Energon_MF` lights them: **red for the Autobots, purple for the Decepticons** (the game's `TnFactionTeamAutobots` and `TnFactionTeamDecepticons` `EnergonColor`, times its `Energon_Intensity` of 10), with the game's pulse running through them (its `fractalMaps` panned across the part, now and then flaring up to three and a half times as bright). Their own lights, Jazz's cyan visor and the like, glow in their painted colour (`TR_Emissive_MF`). In a match everyone wears their map's team's: purple on the Decepticon maps (Downfall and Oblivion), red on the Autobot ones (Ancients and Ignition); on the Customize screen the previews glow red, and purple with **Toggle Faction**, with the build's Decepticon colours
- **The game's pause menu** (Esc), from its multiplayer pause screen: PAUSED with Resume Game, Choose Character, Settings and Quit Game (the game's Switch Teams and Invite Friends left out), and for the host **Return to Lobby** above Quit Game. Quit Game asks first with the game's Quit Game box: "You will be returned to the main menu. Do you want to proceed?" with Yes and No. Return to Lobby asks the same way, then ends the match and brings every player back to the game's lobby, still connected, for the host to start the next. Also from the game: the TELETRAAN 1 / CYBERTRON DEFENSE NETWORK logo with its emblem, hexagons and corner pips animating in, and its sounds. As in a match, the game doesn't stop: the enemies keep flying and Jazz stays in the world, but takes no orders. The world turns into the game's Teletraan hologram: a ring sweeps out from Jazz, leaving a dark blue picture with scan lines where every edge glows blue and far things are blurred, and sweeps the real picture back when you resume (the game's own post-process material, rebuilt as a shader)
- **Weapon upgrades, as the game's multiplayer has them:** each weapon of a Customize build carries one of its three upgrades (Loadouts' "Select Weapon Upgrade"), with the game's multiplayer numbers (`TransWeapons.ini`'s buffs and their `FloatModifiers`; `src\upgrades.rs`): **Ion Cartridge** (more rounds a clip), **Amplifier Pak** (more in reserve), **Hyper Interlink** (faster reloads), **Cyber Optics** (less spread), **Magma Dampener** (spread settling sooner), **Sonic Blast** (faster fire), **Dropshot** (the Scrapmaker's barrels spinning up faster), **R-Infinity Finder** and **Energo-Cell Conductor** (longer range), **Zero-G Casing** (moving faster while firing), **Tachyon Transmitter** (and while charging the Gear Shredder), **Quantum Stabilizer** (less kick), **Photon Accelerator** (the Photon Burst Rifle firing burst after burst), **Omega Pulse Charge** (a quicker lock-on), **Transwarp Ballistics** (faster rounds), **Cyberthermic Frag** and **Cosmic Rust Disperser** (wider blasts), the Chaos-Rift Combustor's extra clusters, **Reaper** (the Energon Harvester draining more), **Torque Ripper** (blades loading faster), **Corrosion Accelerant** (a longer slow), **Orbital Trace Hack** (marks lasting half as long again) and **Energon Stinger** (marks hurting more), **Deadeye** (a Nucleon Charge Rifle hit makes it charge in a third of the time for 5 s: the scope's three lights show it), **Cyber Tactical Scope** (the Photon Burst Rifle's scope showing what you aim at: its name and health, and the range finder), and the special shots: **Lights Out** (the Path Blaster's last round of each clip is a shockwave doing 450 damage within 15 m, with its own flash, tracer, blast, sounds and a hard jolt of the view; the ammo bar's last segment flashes for it), **Planet Buster** (the Riot Cannon's last round flies as a big ball of lightning doing 500 damage within 20 m) and **Fireblast** (a quarter of the Scatter Blaster's shots set what they hit on fire, 15 damage a second for 2 s, and an enemy that dies burning blows up, hurting other players nearby). Each upgrade's part goes on the gun, in the build's colours, in a match and in the Customize preview, where choosing one makes the part glow in its energon colour as it goes on (the game's highlight). The upgrades open with the class's level, as the game's do (below). Escalation's upgrade consoles are gone from the maps (their models and collision), as the upgrades are now the builds'
- **Experience and class levels, as the game's multiplayer has them** (`TnXpManager` and its award classes, `PC-TransGame.ini` and `TransGame.int`; `src\xp.rs`): each class earns experience in a match for the class you play: **a kill by what was killed** (2 for a drone, 10 for a Scrapper or Swarmer, 20 for a Soldier, 30 for a Rocket Trooper, 40 for a Sniper, Sharpshooter or Flyer, 50 for a Shotgunner, 100 for a Heavy Gunner, 150 for a Guardian, 200 for a Leaper and 250 for a Titan, times 0.75 on Easy and 1.25 on Hard, rounded down; the game's multiplayer gives 50 a kill), and the game's awards on top as they are: **First Blood** (100), 3, 5 and 7 kills without dying (100, 150, 200), **Double, Triple and Quad Kill** (100, 150, 200, kills at the same moment), **Just a Flesh Wound!** (75: a kill while under 50 health), **Far and Away** (50: a kill beyond 80 m), **Bangers and Mash** (50: a melee kill of an enemy your EMP grenade disabled), an **Assist** when someone else kills an enemy you marked with the Orbital Pistol or disabled with the EMP grenade (half what the kill gives, by what was killed and the difficulty, at least 1: the game's 25 is half its 50 a kill), **Rough Start** (75: the first death of the match), **Visionary** (50: three of your Orbital Pistol marks out at once) and **En Fuego** (50: 500 damage of Fireblast's fire in one life). Each shows over the crosshair as the game's HUD shows it (`mc_pointTransaction`): "+50", the award's name typing in and its description, with the reward sound; a kill's awards add up in one "+N" two seconds apiece (the description saying what each added), then it rises and fades away. The levels are the game's, 0 to 25 (500 experience for level 1, then 1,500, 3,000, 5,000, 7,500 ... 355,000 for 25); reaching one brings the HUD's **LEVEL UP!** notice with the level and the class's icon, the level-up stinger, and 1,000 shards (2,000 for the first). A class's level opens its weapons, upgrades and abilities (their `RequiresLevel`: the Nucleon Charge Rifle at 6, Deadeye at 7, the EMP grenade at 11, the Subsonic Repeater at 15, Lights Out at 19 and so on) and its second and third builds (at 5 and 10). New builds start with each class's multiplayer loadout (the Infiltrator: Scatter Blaster, Path Blaster and Cloak; the Destroyer: Riot Cannon, Thermo Rocket Cannon and Diffraction Barrier; the Titan: X18 Scrapmaker, Energon Harvester and Whirlwind; the Scientist: Neutron Assault Rifle, Orbital Pistol and Heal Beam) and, as the game's do, a random part of each kind you can wear and random colours, and a saved build holding something its class's level hasn't opened gives it up for the starting one (an upgrade or ability upgrade for none). Every class starts at level 0. A match's experience counts at once (its levels open what they open straight away) but is only kept when the match ends: when the host returns everyone to the lobby, or when online play stops because the host left. Quitting the match (Quit Game, which warns of it, or closing the demo mid-match) loses it, as the game's multiplayer does. The experience and shards are kept in `progress.txt` next to the `assets` folder: lines like `xp<Tab>Scout<Tab>600` (the Infiltrator; `Leader` is the Destroyer, `Soldier` the Titan) and `shards<Tab>2000`, with `owned<Tab>ScoutHead6` for each part bought and `prime<Tab>1` once Prime Mode is on, and signed: a file changed by hand is set aside and the progress starts over, with the game's notice for a damaged profile (see *The progress file* below). The game itself gives no experience in Escalation (`TnGameReplicationInfoSurvival.CanGainXp`); the demo gives multiplayer's in its matches, so the levels can be earned. **Prime Mode**, the game's: once all four classes are at level 25, the party lobby's **Prime Mode** asks the game's question ("Activating Prime Mode will reset all of your current XP ...", Confirm or Cancel); confirmed, it is on for good: every class's experience goes back to 0 (the shards and the parts bought stay, and the levels bring their shards again as they come back), every build starts over (its name kept), the Prime heads open, and the game's Prime medal shows by your name in the lobby's player list, for the other players too
- **The game's Choose Character screen**, from its multiplayer (`UI_GFxCustomize_p.CustomTransformers_GFX`, as a match plays it): CHOOSE CHARACTER over the game, the builds made on the Customize screen listed under their classes (three each, with the class's progress through its level; a slot the class's level hasn't opened reads "Unlocks at level 5" and can't be chosen), and the focused build's loadout beside them: its vehicle, name and level, both weapons with their wireframe pictures and upgrades, and its ability and upgrade, typing and fading in as the game's do, with the game's selection bar, glows and sounds. It comes up as a match starts, over the map seen through the level's own camera (you choose before you play: there is no Jazz on the map yet), and from the pause menu's **Choose Character**. Every class's builds can be chosen, each with its own two weapons (the demo has all sixteen of the game's, below). Jazz takes up the chosen character's two weapons: as the match begins (when the starting scoreboard's countdown runs out, below) or, chosen from the pause menu, the next time he respawns, as the game's notice at the bottom right says ("Selected character used on respawn."). Every respawn brings him back with the chosen character's weapons, as the game gives a respawning player his character's weapons afresh
- **The game's Controls settings** (SETTINGS > Controls), as its PC version has them: Active Game Controls, Keyboard/Mouse Layout, Controller Layout, Controller Vibration, Toggle Vehicle Boost (Hold or Toggle), LS / RS Button Delay, Robot, Vehicle and Jet Camera (Normal or Inverted) and the Camera and Mouse Sensitivity sliders. **Keyboard/Mouse Layout** picks one of the game's keyboard layouts (Default, Annihilator, Quickdraw, Wheelless, Rebel and Southpaw) and lists its keys for the robot and the vehicle in the game's tables. The game only lets you pick a layout; here any key in the tables can also be changed (choose its row and press the new key), which makes a **Custom** layout of your own. The keys in use are the ones the game plays with
- **Jazz** in robot mode with the guns of the character you choose (above) and an over-the-shoulder camera (mouse look, strafing, sprint, jump); the view looks from 60° down to 55° up, as every class's over-the-shoulder camera does in the game (`TnOrbitRotationCameraBehavior`'s `PitchRange`; it used to stop at 34° up)
- **Sprinting, the camera changes as the game's does** (`src\robot_cam.rs`): each class's over-the-shoulder strategy has its own entries for the sprint's camera state (`TnPCS_Sprinting`: the Infiltrator's `Car_MP_OverTheShoulder_STRATEGY`, the Destroyer's and Titan's `Truck_OverTheShoulder_STRATEGY`, the Scientist's `Plane_OverTheShoulder_STRATEGY`), and the demo now uses them. The view widens from 80 to 90 degrees over 0.4 s. The camera comes in closer and drops lower, further out to the side, so he runs low on the left of the picture with more of the way ahead in view: the Infiltrator's 5.5 m out instead of 5.75, aimed 1.8 m to the side and 1 m up instead of 1.65 m and 1.5-1.95 m; the Destroyer's and Titan's 8 m instead of 8.75, aimed 2.95 m to the side; the Scientist's 6.25 m instead of 6.5, aimed 2.45 m to the side. It eases there over about half a second and back as the sprint ends (each strategy's own transition times). Sprinting, the view is kept within 25 degrees of level (the Scientist's keeps its usual range), and the right stick turns it more slowly (below). The view wiggles as he runs: its turn drifts to a new spot picked at random several times a second, up to 0.6 degrees each way (the Destroyer's and Titan's 0.4 across and in roll, 0.25 up and down). The wiggle is as big as his speed allows (`TnWiggler3CameraBehavior`'s `FullWiggleSpeedThreshold`), so it stops when he stands still. Moving at any other time, the view wiggles by a tenth of a degree (the strategy's default profile), as the game's does. The game adds no blur or shake to the sprint, and neither does the demo.
- **Animations from the game:** idle stance, strafing in every direction with all ten of the game's strafe clips for each stance (jogging with the gun ready at the hip, walking while fine aiming): straight forward and back, the four diagonals, and the sides twice, hips forward on the front half and hips turned back on the back half, flipping over as the movement swings round the side as the game's strafe blend does, sprinting (the game has one sprint clip, `Nav_Sprint_F`, which it plays for every sprint, and its robots always sprint the way they face, so sprinting forward and to a side Jazz turns to face the way he runs; held sideways or backwards, Shift leaves him jogging), and Jazz's own transformation animations
- **Turning on the spot, as in the game:** standing still, Jazz's feet stay planted while you turn the camera; his upper body and gun turn to follow it (the game's aim offset, below), and once the camera is about 75° round he steps round to face it with the game's pivot clips, with a footstep as each foot comes down. Each stance has its own 90° turns left and right, and he can spin right round with the 180° turns. A quick swing of the camera gets one big turn instead of several small ones
- **His gun arm points where you aim, as in the game:** his upper body and gun arm turn and tilt to the crosshair, up and down as well as side to side, from the hip and in fine aim alike, over whatever he is doing (standing, jogging, jumping, dashing). It is the game's own aim offset node from the robots' anim tree (`Robot_ANIMTREE`'s `TnAnimNodeAimOffset`, every class's): the offsets the game baked for each bone from its aim poses (`Aim_Shoot_*` from the hip, `Aim_FineAim_*` in fine aim, the hover's while the Hover Slam hovers), put on his bones as the game does, eased in at its own rate. From the hip his gun comes all the way up as the view does to 62° above level and all the way down at 54° below (the profile's `PawnAimOffsetRange`), in fine aim at 54° up and right down; with the gun in his left hand it all mirrors. A blow, a throw or a hit reaction takes it off while it plays, as the game's slots over the node do
- Press **F** to transform: Jazz folds up using his in-game animation, at the game's own speed, while the car's own transformation animation brings its parts together around him, then drives off as his Cybertronian car; F again and the car flies apart as he unfolds back into robot mode, yours to move as he does
- **Jazz's car moves like the game's:** as in the original, the car **hovers** by default: its wheels turn flat into jets, it floats on them, faces where you look and **strafes in any direction** with W A S D. **Hold the right mouse button to boost** (or press it to start and stop, with Toggle Vehicle Boost): it drops onto its wheels and races flat out, **the mouse steering it** as in the game's PC version (its turn is the steering, smoothed by halves a frame; full lock at 550 of the game's mouse units a frame), and the camera swings round after it; it rises back onto its jets when you let go, drifting for a moment before its grip returns. On the wheels it drives as the game's car physics does, a rigid body on four tyres: the throttle held full (its push falling off towards its top speed, with an extra kick from a standstill, against the drag, the two meeting at its top speed: Jazz's car is past 20 m/s within a second and near its 31 m/s top within four, as the game's car runs down Ignition's long corridor in Escalation), the front wheels turning up to 15 degrees by the square of the steering, and each tyre gripping against its own sideways slide, so it corners as hard as its tyres let it (about 100 to 120 degrees a second flat out at full lock) and slides wide when pushed; it leans out of the turn (up to about 15 degrees at full lock). Hovering it strafes at 15 m/s. These are the numbers Escalation gives the cars (`Car_Physics`, `HoverCar_Physics`: the game picks each vehicle's physics by the kind of game, and Escalation takes the second set each vehicle lists, not the first that a multiplayer match takes, `Car_MP_Physics` and `HoverCar_MP_Physics`), and the car falls at the rigid bodies' gravity (`WorldGravityZ`, 29.4 m/s²), not the robots'. A head-on crash while boosting knocks it back to hovering for a second. Off the ground the wheels keep their speed and only the car physics' gentle air control turns and slides them; the hover car can still strafe. All of it follows the game's own vehicle scripts (`TnCarForm`, `TnHoverCarSimulation`, `TnCarSimulation`, `TnWheelAssembly`, `TnTire`) and numbers
- **Jazz's car's camera follows it as the game's does:** Jazz's own camera set in Escalation (`Car_Jazz_CAMSET`; every vehicle's camera is its Escalation character's, below), hovering `HoverCar_Jazz_STRATEGY` (80 degrees wide, 5 m out, 5.75 m strafing, anchored 1.25 m over the car's middle in the view's axes and lower looking level, the view within 20 degrees of level) and boosting `Car_Jazz_STRATEGY` (90 degrees, 5 m out, 1.5 m up less 0.6 m looking level, so the camera rides low behind the car, the whole car in the picture), blending from one to the other over each one's transition time. Hovering, the mouse turns the view as the game's camera turns a vehicle's under the mouse (`TnOrbitRotationCameraBehavior`: the mouse's input held within 1.5, at the strategy's 50 degrees a second across and 25 up and down, times 3.5 with its ramp in full and the Camera Sensitivity's multiplier): as fast as the mouse goes, but no faster than 328 degrees a second across and 164 up and down at the middle Camera Sensitivity. Boosting, the view follows the car round (`MatchRotationSpeed`), so it swings across the picture in a turn, while the mouse still pitches it. The camera states the game's car puts you in change it as they do: in a hard turn (the steering past three quarters of its lock for a quarter of a second) the view widens to 95 degrees, slips out to the turn's side and rolls 2.5 degrees into it; at speed (past a quarter of the top speed for 1.5 s) it drops lower and back; jumping and strafing have their own. The view wiggles slightly as the car moves (`TnWiggler3CameraBehavior`, as `HmWiggler3Data` runs it: its yaw, pitch and roll each drifting in a straight line to a new spot picked at random within a tenth of a degree or so, a new one every eighth to tenth of a second), its turn is smoothed (`HmOrbitSmootherCameraBehavior`), jumps and landings shake it (the car's `JumpingShakeParams`, `LandingShakeParams`), and **boosting blurs the picture as it moves** (the driving strategy's motion blur: `MotionBlur_Amount` 1, each frame's motion). The Destroyer's trucks follow their own (`Truck_MP_CAMSET`).
- **Jazz's car animates like the game's:** the hover poses blending by the way it moves, the hover bob, the switches between jets and wheels, and on the wheels front wheels turned by the car's own steering, the body leaning out of turns (`TnCarSimulation.UpdateBanking`: towards 20 degrees at full lock, pulled against its suspension's springs), and sprung suspension (the wheels drop when the car leaves the ground and the body sinks onto them when it lands). Jumping (Space) fires its jet blast and lifts the nose, with a sharp kick when hovering that the car rights in mid-air; the hover jets burn under it with their glow, sparks and **heat haze** rippling the ground beneath, dying down as it falls and flaring as it takes off and lands; landings throw up sparks and dust (a lighter one for the hover car); the guns fold out of its flanks when it fires (hidden until it first does, as the game hides a vehicle's weapon until then) and away again 3 s after the last shot, or as they reload (`TnWeaponVehicle`'s `PutAwayDuration`); its boosters burn behind it while it's on its wheels, in its energon colour, their heat haze warping what's behind them (yellow for a moment as it jumps on its wheels). All from the game's car animations (Jazz_VEH_ANIM), its car and hover-car physics and its vehicle effects, every Infiltrator's car and Destroyer's truck with its own (Vehicle effects and heat haze, below)
- **Boost dashes** in four directions (tap Shift; hold it to sprint), as the game's multiplayer classes dodge (`DodgeSpeed` 2000, `DodgeTime` 0.95: 19 m at 20 m/s, whatever was moving him before; a dash in the air flies level, while one on the ground falls off ledges, and jumping out of it ends it, so he jumps as high as ever at his running speed, as the game's `DodgingOnGround` does), **melee** as the game's multiplayer has it (every class swings its own sword, the Infiltrator's and the Scientist's **Ion Blade** sliding out of the gun in his hand for the swing and back in as it ends, its blow sweeping the space ahead of him from 0.15 s into the swing: 135 damage to each enemy it meets), or a charging punch when sprinting, and a **landing** recovery after jumps, all using the game's animations
- **Grenades** (the game's frag grenade model) thrown with Jazz's throw animation; they blow up on impact or near an enemy, with the grenade's own explosion effect from the game, and hurt everything close by
- **The Infiltrator's EMP Grenade** (its ability, the game's `FlashBangAbility`), for every Infiltrator build that has it: **hold Q** to see the arc it will fly (the game's trajectory effect, bounces and all), **let go** to throw it. It bounces, goes off a quarter of a second after it first lands and knocks out every enemy within 15 m for up to 4 s (they would stop shooting and turning, crackling with the game's EMP sparks). The meter on the HUD (bottom right) refills over 15 s; the upgrades work as in the game (Short Fuse, EMP Intercooler, Speed Redirector). Its effects, textures, mesh and sounds are the game's own (see *The EMP grenade* below)
- **The other class abilities**, as the game runs them, for every build that has one: the Infiltrator's **Cloak**, the Destroyer's **Diffraction Barrier** and **Hover Slam**, the Titan's **Whirlwind** and **Flak Shield**, and the Scientist's **Heal Beam** and **Battle Sentry**, each on **Q** with its meter on the HUD, its upgrades, clips, effects, models and sounds from the game (see *The class abilities* below)
- **Jazz's shield and health:** Jazz has the game's **shield** over his health: 100 points of shield for every class, over his class's health as the game's multiplayer gives it (the Infiltrator and the Scientist 300, the Destroyer 400, the Titan 500: `TR_Health_p.MP_<class>_Health`; Escalation's own data gives every class 100): hits wear the shield down first, and once he has gone 4 seconds without damage it recharges at 50 points a second (in the car, after 7 seconds at 12 a second, as the game's car form has it), with the game's sounds as it runs out and as it fills up again, and the game's shield effects (`src\shieldfx.rs`, `TnShieldMaterialOverrider`): as it starts to fill again after a hit, a sphere sweeps over him and fades (`Shield_Restore_FX`; from empty, `Shield_FullRestore_FX`), as it runs out the sphere breaks (`Shield_Break_FX`), and a shot that strikes it ripples it where it came in (`Squib_Shield_FX`, at most every half second), the sphere round his bones at his skeleton's own size, before his class's size takes it down (his vehicle's body in the car), on the other players' Jazzes too. As in the game, the recharge only reaches the shield: his own health stays down until he respawns or buys health at a health station (below). He flinches when hit, and when his health runs out he dies as the game's soldiers die (below), or, with other players in the match, goes down to be revived (Escalation's downed players, below); the dead come back at their starts, both full, as the team's round ends. (The enemies don't fight yet, so for now only a burning enemy's explosion hurts him: in Escalation a player's own blasts never do.)
- **Deaths as the game plays them** (`src\downed.rs`, `src\ragdoll.rs`, built by `tools\build_deaths.py`): every class's robot dies by the soldiers' death set (`TR_Deaths_p.Sets.Basic_DEATHSET`) and falls as their physics asset (`TR_Shared_PHYSICS_p.RB_Shared_PHYS`). He plays the clip the death set has for how he was killed (shot from the front, one of its 16 gunfire deaths; a headshot its own; the E.D.K. TechVolt's or the Lights Out shockwave's electrocution; the Corrosive Slime Cannon's stagger; the Energon Harvester's reach as he dissolves), cutting in over whatever he was doing; then his robot goes limp as a ragdoll, carrying on as the clip moved it, with the blow's push and a blast's throw (an explosion throws him from the start), while the camera follows his body and looks down at it. Once it has come to rest (and lain as long as the death says, 4.5 s after he died at most) it dissolves out from its middle in the game's cyan, and he comes back once it has gone and the team's round has ended (Escalation's downed players, below). Killed in the air, he falls as he is. His death plays the death set's sparks, lights, sounds, camera shake, severed pieces and gibs as well, as the enemies' do (*How they die*, under *Escalation's enemies*). The other players see their copy of him die the same way, and every PC plays an enemy's death alike (the network protocol is now version 34)
- **Jumping and falling animations from the game:** Jazz jumps the game's 6.5 m, pushing off and tucking into his jump, then dropping into the descent; stepping off a ledge plays his fall. Landings follow his own rules: the ordinary landing after a drop of 2.5 m or more, the heavy one after a 12 m drop at speed or a 45 m one
- **Air dashes** and a **hard landing** after a long time in the air
- **Escalation's downed players** (`src\revive.rs`): with others in the match, running out of health takes you down rather than killing you. Down, you can still look round and fire while your downed meter drains; a teammate holding E beside you revives you, and G held ends it. The dead spectate a teammate until the team's round ends. The team has 3 lives, and losing the third brings up the final scores and then the lobby (Escalation's downed players, revives and lives, below)
- **The game's starting scoreboard** (`UI_GFxPreGameCountdown_p.PreGameCountdown_GFX` with its players' list, `UI_GFxPlayerList_p.PlayerList_GFX`), shown once you have chosen your character, as Escalation shows it: the map through its level camera (the view the game gives the players before they spawn), darkened through the movie's blue gradient, its bands of light sweeping out, the game mode's icon turning in at the left, at the top right **WAITING FOR PLAYERS...** until every player in the match has chosen his character, then **MATCH STARTS IN** counting down **15 seconds** (each number below 10 popping in with a white glow, each second from 10 down ticking), and the scoreboard: DECEPTICONS (on Downfall and Oblivion; AUTOBOTS on Ancients and Ignition) with the team's score, the columns LEVEL (the symbol of the class each player has chosen and his level in it, as on the scoreboard in a match), NAME, SCORE, KILLS, DEATHS, REVIVES and MONEY, and a row for each player with his connection's bars, yours highlighted. Nothing has been scored yet, so the numbers are all 0. At zero the scoreboard goes and the match begins: Jazz is at his start with his character's weapons, the HUD comes up, the enemies appear and the announcer and the match music start. Online, the countdown is the host's and every PC follows it, so it shows the same number everywhere and the match begins for everyone together; no Jazz is seen until then
- **The game's scoreboard in a match** (Tab, `UI_GFxInGameStats_p.InGameStats_GFX`), over the game as it plays on: the view darkened, ON DOWNFALL (or the map you're on) and "Survive waves of attacks" at the top, and the players' list with each player's **LEVEL** (the symbol of the class he plays and his level in it) in place of the ability, his name, SCORE, KILLS, DEATHS, REVIVES and MONEY, as each player's match stands, ranked as the game ranks them. Tab again, or Esc, puts it away
- Shooting in both forms, with a crosshair. Jazz starts with the two weapons of the character chosen as the match starts (Choose Character, above; without its files, the **Neutron Assault Rifle** and the **Scatter Blaster**; with them, the rifle lies on the floor off to his right, below) and swaps between them with the mouse wheel; in the car he fires twin nose guns (the M1 Tachyon Repeater, with the multiplayer's 60-round clip and 360 in reserve, out to 200 m). Fire rates and damage are the game's own values (rifle: one shot every 0.10 s, 600 per minute, 18 damage; Subsonic Repeater: every 0.065 s, 12 damage; car guns: every 0.075 s, 15 damage; Nucleon Charge Rifle: a shot every 1.25 s at most, 75 damage, charging up to 150 and 285 in its scope; Path Blaster: every 0.385 s, 67 damage; Riot Cannon: a round every 0.85 s, 155 damage to everything within 6.5 m of where it goes off; Orbital Pistol: every 0.85 s, 100 damage, marking what it hits; X18 Scrapmaker: a round every 0.35 s easing down to one every 0.065 s as its barrels spin up, 27 damage; Energon Harvester: bursts of two shots 0.2 s apart, each nine 20-damage pellets; E.D.K. TechVolt: a beam doing 120 damage a second; Corrosive Slime Cannon: a round a second, 125 damage to everything within 8 m of where it goes off; Gear Shredder: up to three saw blades a shot, 165 damage each; Thermo Rocket Cannon: a rocket at a time, 375 damage to everything within 7 m; A-4 Pulsar Cannon: a round a second at most, 225 damage to everything within 7.5 m; Chaos-Rift Combustor: a bomb at a time, 150 damage to everything within 4.5 m and 150 within 8.5 m for each of its five clusters). Holding the trigger, the second shot waits the whole interval after the first, as the game's refire timer has it (it used to come after half of it when the gun had been resting). As in the game, every gun but the Riot Cannon, the Corrosive Slime Cannon, the Gear Shredder, the Thermo Rocket Cannon, the A-4 Pulsar Cannon and the Chaos-Rift Combustor (whose rounds fly: below) is an instant-hit weapon: a shot strikes the first thing in its line the moment it is fired (out to the game's ranges: 300 m for the rifle, drones, Nucleon Charge Rifle, Path Blaster, Orbital Pistol and X18 Scrapmaker, 200 m for the car guns, 150 m for the Subsonic Repeater, 40 m for the Energon Harvester, 27.5 m for the E.D.K. TechVolt), and its tracer flies out at the game's tracer speed. Every gun fires where the crosshair points, as the game fires them: each shot is traced along the camera's line, from the point of it level with Jazz (`TnPlayerPawn.GetWeaponStartTraceLocation`), so it strikes whatever is under the crosshair however near it is, while its tracer (and the rounds of the guns whose rounds fly) leaves the muzzle for where it struck. Only when his gun is up against something (anything within 2.5 m of the muzzle along the view) does the shot set off from the muzzle instead, so he can't fire through a wall. The car's guns work the same way: their shots are traced along the camera's line from the point of it level with the car, so looking up, down or across sends the rounds there, and the tracers leave the barrels for the point under the crosshair. Firing kicks the camera up the way the game's weapon data has it (`CameraRecoils`, played by its recoil camera): every rifle shot lifts the view 0.4 degrees (0.1 in fine aim, where the field of view also widens by a degree and settles back after you stop), so it climbs as you hold the trigger and you pull it back down, and jolts it, from the hip and in fine aim alike (its `CameraShakes`, played by the camera's shaker with the shake waves of Jazz's own over-the-shoulder camera, `Car_OverTheShoulder_STRATEGY`: a tremble of up to about a quarter of a degree that dies away within a twentieth of a second, the HUD jolting with it as the HUD movie's `updateCameraShake` does). The game snaps the view to each kick; the demo eases it in instead, so the camera moves smoothly: a kick comes in over about a fifth of a second (most of it in the first tenth, the field-of-view change likewise), with the same total climb. The jolts are the game's frame for frame at 60 frames a second (measured against videos of the game); above that, each comes in over the first sixtieth of a second rather than all at once. The rifle's spread is the game's too: 0.05 to 0.10, opening by 0.005 a shot while settling back all the time at the rate that closes it in 2 s, so held down the crosshair's prongs open over about two seconds; half that in fine aim, and 1.25 times it in the air (`TnWeaponSpreadModifier`), as for every gun. Its rounds do 1.15 times their damage out to 25 m, falling to a quarter at 150 m (its `RangeDamageScale`). Its two barrels take turns: each shot's flash, tracer and round leave from the next one (the gun's `MuzzleFlash` and `MuzzleFlash2` sockets); the Scatter Blaster kicks hardest (below). The Subsonic Repeater's, the Photon Burst Rifle's, the Orbital Pistol's and the car guns' camera recoil is zero in the game, so they don't kick; but every gun jolts the view with each shot the way the rifle does, at its own strength (its `CameraShakes`): the Subsonic Repeater a little harder than the rifle, the Photon Burst Rifle harder still with every round of a burst, the Scatter Blaster harder (up to about two thirds of a degree, dying away over a third of a second), the Nucleon Charge Rifle and the Riot Cannon harder still (below), and the Orbital Pistol and the car's guns barely at all
- **Shooting and fine-aim animations from the game:** Jazz's gun is always up and ready at his hip (the game's shooting stance, with its own jog in every direction: Escalation never lowers it, as the game's weapons only drop to their relaxed stance after a while unused in the campaign). While the trigger is held the arm recoils and the rifle's barrels spin; letting go spins them down. **Hold the right mouse button to fine aim**: Jazz brings the gun up to his sights and walks, and the camera zooms in from the game's 80 to 50 degree field of view, with slower mouse turning. The zoom eases in and out as the game's camera does (its smoother, a critically damped spring: in over its 0.1 s and back out over 0.15 s, half way in after some 0.04 s), for every weapon. The Subsonic Repeater gets its own firing animation the same way
- **Switching hands, as in the game (C):** Jazz moves his gun to his other arm and the camera swings over to that shoulder. His whole animation is mirrored left for right, blending across over half a second the way the game's handedness mirror does, while the gun folds back into one forearm and unfolds out of the other (each weapon's `MirrorReverse_WEP` clip and its own change-hands sound) over a second, when he can't fire or aim. His forearms change over with it, as in the game (his arm parts' `ADD_Weapon_MirrorReverse_ARM`): the bare forearm on the arm the gun goes to folds up and shrinks away piece by piece, its hand drawing in, while the one on the arm the gun leaves unfolds out of the folded gun and grows back, its wheel last, so neither arm is ever left empty from any camera angle. Every move, strafe, turn and weapon animation works on either side: on his left arm the guns are the game's left-handed weapon meshes, and his right forearm comes back. As in the game he can't switch while dashing, meleeing, transforming, changing weapons or sprinting (the gun is put away while he sprints, as in the game: *Putting the gun away to sprint*, below)
- **Switching weapons animates as in the game:** turning the mouse wheel (Swap Weapons) folds the gun in hand back into Jazz's forearm while the other one unfolds out of it, both at once over a second, with each weapon's own equip and holster animations and sounds (the game's `TnWeaponEventEquip` / `UnEquip` effects). Picking up a weapon unfolds it the same way. As in the game, the new gun can fire 0.2 s after the switch (its `EquipTime`), while it is still unfolding
- **The Photon Burst Rifle**, lying next to the start point the way the game shows weapons on the ground: the weapon held still just off the floor, side on, with a glowing cyan outline. Walk up to it and **hold E** ("Hold E to pickup Photon Burst Rifle", the game's own prompt, in its contextual-command line at the bottom of the HUD) to swap it for the weapon in your hand, which is left in its place with its ammo; swap back the same way. Jazz carries two weapons at a time and swaps between them with the mouse wheel. It is the game's weapon (`BurstRifle_SV_WEPDATA`): three-round bursts, one per pull of the trigger (0.1 s apart, 0.325 s between bursts), 32 damage, 300 m range, 15-round clips (105 in reserve, 2 s reload), with its own mesh, textures, recoil, drum and reload animations, sounds (burst, tail, equip, reload, scope, dry fire, pick-up), blue muzzle flash, tracers and impacts, its tactical reticle, and a **5x scope**: fine aim zooms to the game's 30 degrees and brings up the HUD's tactical scope with its range finder and compass
- **The Scatter Blaster**, Jazz's second gun as a match starts when there is no Choose Character screen, and one of the Infiltrator's weapons (turn the mouse wheel to bring it out; swapped for a weapon on the ground, it lies there in its place). It is the game's automatic shotgun (`Shotgun_SV_WEPDATA`): hold the trigger and it fires every 0.425 s, each shot seven 12-damage pellets in the game's fixed spread pattern (tightened to three quarters in fine aim), each pellet with its own tracer and impact. Damage falls off with range as the game's does (`RangeDamageScale`): double up to 20 m, down to a quarter at 50 m, out to its 65 m range. 8-round clips (40 in reserve, 2.5 s reload), 2 rounds left brings the low-ammo shot sound. It has its own mesh and textures (from `WEP_Shotgun_p.xxx`), its arm recoil and moving parts per shot, idle, reload, equip, holster and change-hands animations, the camera kick the game gives it (the view jumps up 2.75 degrees a shot, 1.375 in fine aim, and the field of view widens by a degree and settles back), its sounds (shot, tail, low-ammo shot, equip, holster, reload, change hands, fine aim, dry fire, pellet impacts on walls and enemies), its fiery muzzle burst at the end of its barrel (its `MuzzleFlash` socket), where its pellets and tracers leave from too; per pellet a fan of long fiery streaks that race out ahead of the pellet (stretching to twice their length as they go, as the rifle's do) with dark smoke flying along its line, all cut off where it strikes; spark-and-smoke impacts; a big brass shell with every shot, its open end glowing energon cyan, popping out of the shell port on top of the gun, which steams after each shot; steam rising by Jazz's wrist as it reloads (its `ReloadFX` socket); its HUD icon and the game's shotgun reticle, whose three arcs sit out at the spread and stretch with it
- **The Subsonic Repeater**, lying on the floor off to Jazz's left as a match starts (5 m to his left and a step ahead, side on with the same cyan outline; hold E there to pick it up in place of the gun in his hand). It is the game's rapid-fire energy gun (`TnWeaponIonBlaster`, the game's "IonBlaster" inside; `IonBlaster_SP_WEPDATA`): hold the trigger and it fires a 12-damage instant hit every 0.065 s (over 900 a minute) out to 150 m, worth 1.35 times that up close and falling off from 20 m to a quarter at 100 m (its `RangeDamageScale`). 60-round clips (300 in reserve, 1.25 s reload); 5 rounds left brings the low-ammo shot sound. Its spread is the game's: a steady 0.11 that doesn't open as you fire, from the hip, and 0.65 times that in fine aim (its `FineAimSpreadModifier`), where the camera zooms in to the game's 50 degrees as for the rifle, with the rifle's reticle. As in the game it has no camera recoil, so the view doesn't climb, but every shot jolts it a little (its `CameraShakes`: intensity 0.25 with a slight roll, up to about a third of a degree and gone within a twentieth of a second), from the hip and in fine aim alike, so held down the view trembles slightly, as in the game. It has its own mesh and textures, its firing, idle, reload, equip, holster and change-hands animations, its sounds (shot, tail, low-ammo shot, equip, holster, reload, change hands, fine aim, energy impacts on walls and enemies, and the electric crackle of its dry fire on an empty clip), its HUD icon and its effects (below): with every shot a white-blue starburst flash at both of its barrels with a pale blue light (its rounds leave the upper one), a shell from the port under its drum for one shot in four, two thin cyan bolts and a squiggly glowing trail flying out at the game's 200 m/s, and where a round strikes, a dark burst with a blue flash, white-blue bits and sparks, leaving a bullet hole whose rim glows cyan; as it reloads it steams by Jazz's wrist
- **The Nucleon Charge Rifle**, lying on the floor behind Jazz on his left as a match starts (the lower left as he looks over it, 3 m behind him and 3 m to the left, side on with the same cyan outline; the Riot Cannon lies where it used to, ahead of him on his left; hold E there to pick it up in place of the gun in his hand). It is the game's charge sniper rifle (`TnWeaponSniperRifle`, the "SniperRifle" and "Null Ray" inside; `WEP_SniperRifle_p`'s `SniperRifle_WEPDATA`): semi-automatic, each pull of the trigger fires one instant hit, the next no sooner than 1.25 s later (a pull meanwhile fires as soon as it can), out to 300 m, at full damage to 150 m and falling to 0.15 times it at 250 m (its `RangeDamageScale`). From the hip a round does 75 damage, with no crosshair (the game shows none for it: its `ShouldShowReticule` is off) and a spread of 0.08 that opens by 0.04 a shot up to 0.16 and settles back over 3 s. **Hold the right mouse button** and its scope comes up, as in the game: the camera moves in close behind Jazz's head (the sniper offsets of his over-the-shoulder camera, `Car_OverTheShoulder_STRATEGY`: 1.75 m behind the middle of his body, 1 m to the side and 2 m up, moving in over 0.1 s and back out over 0.5 s on the camera's smoother) and zooms to 25 degrees, the HUD shows the game's sniper scope (below), and shots go dead straight (no spread at all). As the camera moves in to the scope Jazz fades out of the picture, and back in as it leaves: the game's scope leaves him out of its view, but bigger armor (Snarl's) would reach into it. The game also fades the player's own character whenever the camera comes near him (`TnCamera.UpdatePawnTransparencies`), and so does the demo: his collision cylinder padded by 0.3 m (the camera strategy's `PlayerCylinderRadiusPadding` and `PlayerCylinderHeightPadding`) is taken as a capsule, and the camera's distance from it over 2 m (`PlayerFadeStartDistance`) is how much of him is drawn, in a fine dither pattern (with the camera pushed in against a wall, less of him). Jazz holds it from the hip meanwhile and jogs rather than walks (its `FineAimStance`). While the scope is up and the rifle ready, it **charges**: a level every 0.75 s, a round doing 150 damage after the first and 285 after the second (its `ChargeDelay` and `InstantHitDamage` by fire mode), the scope's charge readout counting up to 100% and turning to "ready"; fully charged, red rings of light run back along the barrel. The charge starts over when the scope comes up or goes down and when the rifle is ready again after a shot, a reload, a swap or a switch of hands (its `ResetFireMode`), so each shot needs its own charge. Every shot kicks the view up 1.5 degrees, from the hip and scoped alike (its `CameraRecoils`), and jolts it hard (its `CameraShakes`: intensity 0.55 with a roll, over 0.45 s). 4-round clips (20 in reserve, 2 s reload); 2 rounds left brings the low-ammo shot sound. It has its own mesh and textures, its firing animation (the gun's with the arm's heavy recoil), idle, reload, equip, holster and change-hands animations, its sounds (shot, low-ammo shot, dry fire, reload, equip, holster, change hands; the scope's whine and hum with crackles of static for as long as it is up, the charging tone rising in pitch, the power-up and beeps once fully charged, and the zoom out; its own impacts on walls and enemies), its HUD icon and its effects (below): a blast of orange flame from the barrel into a big rolling cloud of fiery smoke, with a red light; three cyan bolts racing out at over 900 m/s with rings hanging round the shot's last few metres; and where it strikes, white-hot flares, sparks, a burst of fire, a lingering glow, metal chips and smoke. With every shot a big shell pops out of its shell port, which steams, and it steams by Jazz's wrist as it reloads
- **The Path Blaster**, lying on the floor behind Jazz on his right as a match starts (the lower right as he looks over it, 3 m behind him and 3 m to the right, across from the Nucleon Charge Rifle, side on with the same cyan outline; the Orbital Pistol lies where it used to, ahead of him on his right; hold E there to pick it up in place of the gun in his hand). It is the game's heavy blaster pistol (`TnWeaponHeavyPistol`, the "HeavyPistol" and "Battle Pistol" inside; `WEP_HeavyPistol_p`'s `HeavyPistol_SV_WEPDATA`, which Escalation uses): automatic, hold the trigger and it fires a 67-damage instant hit every 0.385 s out to 300 m, worth 1.15 times that up to 20 m and falling to a quarter at 150 m (its `RangeDamageScale`). Its spread starts at 0.04 and opens by 0.02 a shot up to 0.10, settling back over 2.5 s, half that in fine aim, with the game's pistol reticle, whose three prongs sit out at the spread. Every shot kicks the view up hard, 1.5 degrees from the hip and 0.75 in fine aim, the field of view widening by a degree and settling back over half a second (its `CameraRecoils` and `FineAimCameraRecoils`), and jolts it (its `CameraShakes`: intensity 0.35 with a roll, over a third of a second). **Hold the right mouse button** to fine aim: the camera zooms in to the game's 50 degrees over Jazz's shoulder as for the rifle (the game gives it no camera of its own), with the sound of its sights coming up. 10-round clips (40 in reserve, 2.5 s reload); 2 rounds left brings the low-ammo shot sound. It has its own mesh and textures, its firing animation (the gun's kick with the arm's light recoil at 2.25 times its speed, with every shot), idle, reload, equip, holster and change-hands animations, its sounds (shot, low-ammo shot, sights, reload, equip, holster, change hands, the ballistic dry fire and impacts), its HUD icon and its effects (below): a white-hot flash, a fire burst and a fiery ring blowing out of the barrel into a cloud of fiery smoke, with a strong orange light; three red-hot bolts racing out at 180-510 m/s; and where it strikes, a burst of fire, a spray of golden sparkles, flames and smoke, leaving a scorch mark on the wall. With every shot a big shell pops out behind Jazz's wrist, which steams, and it steams under his forearm as it reloads. Its multiplayer upgrades are in, **Lights Out**'s shockwave rounds among them (see the weapon upgrades above)
- **The Riot Cannon**, lying on the floor ahead of Jazz and to his left as a match starts (the upper left as he looks over it, 4.5 m ahead and 3 m to the left, where the Nucleon Charge Rifle lay before, side on with the same cyan outline; hold E there to pick it up in place of the gun in his hand). It is the game's energy cannon (`TnWeaponBazooka`, the "Bazooka" and "Fusion Cannon" inside; `WEP_Bazooka_p`'s `Bazooka_SV_WEPDATA`, which Escalation uses), the one gun here whose rounds fly rather than strike at once: automatic, hold the trigger and it fires a round every 0.85 s. Each round (`TnProjectileBazooka`, with the data Escalation gives it, `Bazooka_SP_PROJDATA`) flies dead straight at the game's 180 m/s, for up to 4 s, and goes off on the first thing in its way (a wall, the floor, a pillar, an enemy, another player): 155 damage to every enemy within 6.5 m, as much at the blast's edge as at its heart and through walls too, as the game's `HurtRadius` does it for this round. The blast doesn't hurt Jazz himself or the other players: Escalation turns a player's own blasts off (its `SelfDamageMultiplier` is 0). Its spread is a steady 0.04, half that in fine aim, with the game's cannon reticle. Every shot kicks the view up 1.5 degrees from the hip (the field of view a degree wider for a moment) and 0.75 in fine aim, each easing back over 0.35 s after a tenth of a second (its `CameraRecoils` and `FineAimCameraRecoils`), and jolts it the hardest of the guns (its `CameraShakes`: intensity 0.65 with a roll, over 0.4 s). **Hold the right mouse button** to fine aim: the camera zooms in to the game's 50 degrees over Jazz's shoulder (the game gives it no camera of its own), with the sound of its sights. 4-round clips (24 in reserve, 2.5 s reload). It has its own mesh and textures, its firing animation (the gun's kick with the arm's heavy recoil, once per shot), idle, reload, equip, holster and change-hands animations, its sounds (shot, sights, reload, equip, holster, change hands, the plasma dry fire, the round's crackling flight loop and its blast), its HUD icon and its effects, which are the game's own particle systems run as its engine runs them (see *The Riot Cannon*, below): a blue-white burst at the barrel with lightning, rings and purple smoke, and a violet light; the round flying as a blazing blue-white ball, flickering with bursts and trailing purple smoke, with its own light; and where it goes off, a white flash with purple rays, fire, smoke, white sparks and embers. It steams by Jazz's wrist as it reloads. Online, its rounds fly and go off on the other players' PCs too
- **The Orbital Pistol**, lying on the floor ahead of Jazz and to his right as a match starts (the upper right as he looks over it, 4.5 m ahead and 4.5 m to the right, where the Path Blaster lay before, side on with the same cyan outline; hold E there to pick it up in place of the gun in his hand). It is the game's marking pistol (`TnWeaponTracerDart`, the "TracerDart" inside: a Scientist's gun in multiplayer, built on the campaign's SysCorruptor; `WEP_TracerDart_p`'s `TracerDart_MP_WEPDATA`): automatic, hold the trigger and it fires a 100-damage instant hit every 0.85 s out to 300 m, at full damage out to 200 m (20,000 of the game's units) and half that beyond (its `RangeDamageScale`, the distance measured as the game measures it, from the middle of Jazz to the middle of the enemy). **Every hit marks the enemy it strikes**: marked, it takes 1.1 times the damage of everything that hurts it (this gun's next shots, the other guns', the Riot Cannon's blasts, the other players' shots), 1.2 times after a second hit and 1.3 times after a third, which is the most (further hits keep it at 1.3); each hit puts its mark on afresh for 10 s (it ends early if the Jazz who last hit it dies). A marked enemy shows it: bright orange arrows keep closing in on it from either side, one a side for the first mark, two for the second and three for the third, turned to face you, as in the game. And for 8 s after each hit it is **revealed** to you and the other players: the game's enemy marker, in the enemy colour, hangs over it on everyone's HUD wherever it is, through walls, with its mark's icon (one, two or three rings) on it. Its spread starts at 0.02 and opens by 0.25 a shot, straight to its widest, 0.15, settling back over a second, so a shot after a short rest is the sharp one; half that in fine aim, with the game's rifle reticle (the game has none of its own for it). It doesn't kick the view at all (its camera recoil is zero in the game), only jolts it slightly (its `CameraShakes`: intensity 0.05 with a roll, over 0.15 s). **Hold the right mouse button** to fine aim: the game's 50 degrees over Jazz's shoulder (the game gives it no camera of its own), with the sounds of its sights coming up and going down. 6-round clips (24 in reserve, 1.25 s reload). It has its own mesh and textures, its firing animation (the gun's kick with the arm's light recoil at twice its speed, with every shot), idle, reload, equip, holster and change-hands animations, its sounds (shot, sights, reload, equip, holster, change hands, the ballistic dry fire, the energy impacts on walls and enemies), its HUD icon and its effects, which are the game's own particle systems (see *The Orbital Pistol*, below): big yellow-green glows at the barrel with a pale cyan light; its tracer, a beam reaching straight to where the shot struck, there at once and gone in a fifth of a second (a thin, blinding yellow-white core in a wide, faint band of panning lightning); and where it strikes, a burst of fire and smoke. While it is ready in Jazz's hand, wisps of dark steam drift from under the gun, and it steams at his elbow as it reloads. Online, its marks, their arrows and the reveal show on every player's PC. (The demo's enemies have 54 health, so a hit within 200 m wrecks one outright: the marks build up on enemies hit from further away, or hurt by something else first)
- **The X18 Scrapmaker**, one of the Titan's weapons (a Titan build with it in its loadout starts the match with it: Choose Character, above). It is the game's heavy machine gun (`TnWeaponHeavyMG`, the "HeavyMG" inside; `WEP_HeavyMG_p`'s `HeavyMG_SV_WEPDATA`, which Escalation uses), whose barrels spin up before it fires: pull the trigger and its fire interval eases from 0.35 s down to 0.065 s over a second of the trigger held (its `FireIntervals`), running back up at the same rate when you let go, and until it is down to 0.15 s it only winds up (`TnWeaponHeavyMG.FireAmmunition`: its wind-up sound and the warm-up turn of its barrels), so its first round leaves about 0.8 s after the pull and from a second on it fires over 900 a minute. Each round is a 27-damage instant hit out to 300 m, at full damage to 50 m and down to a tenth at 100 m (its `RangeDamageScale`), from its two barrels in turn. It holds a 200-round drum and nothing in reserve (the game gives it no `MaxReserveAmmoCount`: the ammo stations fill the drum); 10 rounds left brings the low-ammo shot sound. Its spread opens from 0.07 by 0.0025 a round up to 0.14, settling back over 4 s (0.85 of that in fine aim), with the rifle's reticle (the HUD movie has none of its own for it). Every round kicks the view up a tenth of a degree (half that in fine aim) and jolts it a little (its `CameraShakes`: intensity 0.15 with a roll, over a tenth of a second). Jazz holds it at his hip in the game's **heavy-gun stance** (`STANCE_HeavyGunArmShootFromHip`, which it has from the hip and in fine aim): its own idle, its jog and walk in all ten strafe directions (blending from the walk into the jog by his speed, as the game's anim tree does) and its own turns on the spot, 90 degrees and right round; and he moves at three quarters of his speed while it fires (its `FiringGroundSpeedMultiplier`, from the first pull until the firing stops, wind-up and all). It comes out in a fifth of a second (its `EquipTime`). It has its own mesh and textures, its firing animation (its barrels turning, with the arm's rapid heavy recoil) looping while it fires, its warm-up and cool-down, idle, reload, equip, holster and change-hands animations, its sounds (the wind-up; each shot over the loop held while it fires, and its tail as it stops; low-ammo shot, fine aim, reload, equip, holster, change hands, the plasma dry fire, ballistic impacts), its HUD icon and its effects, the game's own particle systems (see *The Titan's weapons*, below): a fiery flash on the barrel that fired, with a red light; scrap bolts racing out at 120-150 m/s behind a glowing trail; and where they strike, spears of fire, embers and a blast. With every round smoke rolls off its spinning barrels and its shell port throws out a spent shell's glowing trail and steam. Online, the other players see its wind-up, its rounds and its cool-down
- **The Energon Harvester**, another of the Titan's weapons: the game's draining shotgun (`TnWeaponDrainGun`, the "DrainGun" inside, built on the EMP Shotgun's mesh; `DrainGun_MP_WEPDATA`, the data a match gives it: Escalation has none of its own for it). Each pull of the trigger fires a burst of two shots 0.2 s apart, and the next burst comes no sooner than 0.6 s after (its `ShotsPerBurst` and `BurstDelay`); each shot is nine 20-damage pellets in the game's fixed pattern, a plus and an X (its `SpreadPattern`, tightened to 0.8 of it in fine aim), each with its own impact, at full damage to 20 m and none by 40 m, its range. **Every pellet that wounds an enemy gives Jazz 15 health back** (`TnWeaponDrainGun.ProcessInstantHit`: its `HealthPerHit`), up to his full health (the shield recharges on its own), with the game's heal effect: a swarm of glowing health crosses sparkling round him (`Harvester_PlayerHeal_FX` on the gun's `MeleeSocket`, starting over with each shot that heals). 6-round clips (24 in reserve, 2.5 s reload); its last round brings the low-ammo shot sound. Every shot kicks the view up 2.5 degrees (1.25 in fine aim), the field of view narrowing by a degree, easing back over 0.7 s after a fifth of a second (its `CameraRecoils` and `FineAimCameraRecoils`), and jolts it harder than any other gun (its `CameraShakes`: intensity 0.8 with a roll, over half a second). It takes three quarters of a second to come out (its `EquipTime`). It has its own mesh and textures (the EMP Shotgun's, `WEP_EMPShotgun_p`), its firing animation (the gun's kick with its own arm recoil, for each shot of a burst), idle, reload, equip, holster and change-hands animations, its sounds (the shotgun's: shot, low-ammo shot, fine aim, reload, equip, holster and change hands; the plasma dry fire; the shotgun's impacts on walls and enemies), the game's shotgun reticle, its HUD icon and its effects (below): a burst of blue energy at the muzzle with each shot, with a pale blue light; a blue blast reaching out along the shot once a burst; white-hot flashes where each pellet strikes; crackling arcs round the muzzle while it is ready; and steam from its shell port as it reloads
- **The E.D.K. TechVolt**, another of the Titan's weapons: the game's chaining beam gun (`TnWeaponChainBeam`, the "ChainBeam" inside, built on the Repair Beam's mesh; `ChainBeam_SV_WEPDATA`). Hold the trigger and it fires a beam of lightning out to 27.5 m, there for as long as you hold it (a round every 0.07 s), doing 120 damage a second to what it strikes, at any range (`TnWeaponBeam`: its `DamagePerSecond` over each round). **It chains to every other enemy near the beam** (`TnWeaponChainBeam.UpdateChainBeam`): each one within 14 m of it, inside 55 degrees of its line from the muzzle and in sight of it, is struck by an arc of lightning from the beam's nearest point for 80 damage a second (its `ChainDamagePerSecond`). 50-round clips (3.5 s of beam; 200 in reserve, 2.5 s reload). Its spread starts at 0.02 and opens straight to 0.04, settling back over a second (half that in fine aim), with the game's own beam reticle (`mc_crosshairChainBeam`, its three prongs spinning in). It doesn't kick the view (its camera recoil is zero) but jolts it lightly with every round (its `CameraShakes`: intensity 0.1 with a roll), and Jazz moves at three quarters of his speed while it fires. It has its own mesh and textures (the Repair Beam's, `WEP_RepairBeam_p`), its firing animation (looping while the beam is on, with the arm's rapid light recoil), its cool-down, idle, reload, equip, holster and change-hands animations, its sounds (the beam's loop held while it fires, fading out over a quarter of a second, and its last crackle as it stops; a crackle for every round that strikes an enemy; reload, equip, holster, change hands, fine aim, the electric dry fire; its impacts on walls), its HUD icon and its effects (below): the beam itself, a crackling bolt from the muzzle to where it strikes, with its flash; the chains' arcs; sparks where it strikes; arcs playing over its emitter while it is ready; a blue light coming up on the muzzle over half a second as it fires and going down as it stops; and steam as it reloads. Online, the other players see its beam and its chains
- **The Corrosive Slime Cannon**, the last of the Titan's weapons: the game's slime mortar (`TnWeaponAoESlow`, the "AoESlow" inside, on the "RustCannon" mesh; `AoeSlow_WEPDATA`, the data a match gives it: Escalation has none of its own for it). Hold the trigger and it fires a round a second, each one (`TnProjectileSlow`, with its `Slow_MP_PROJDATA`) a glob of slime flying at 70 m/s and **falling as it flies**, under the world's gravity (its `GravityScale` of 1), for up to 4 s, going off on the first thing it strikes for 125 damage to every enemy within 8 m (full to 4 m, half at the edge: its `RadiusDamageScale`), but not Jazz himself (Escalation's blasts don't hurt their own player). **Hold the right mouse button** and instead of fine aiming it shows where its round would go (`TnWeaponAoESlow.TryAltFirePress`: its `TnSimulateProjectileExtension`): the game's trajectory effect (`Slime_Trajectory_FX`) laid along the round's flight, simulated afresh every frame from the muzzle towards what the crosshair is on, a twentieth of a second at a time for up to 3 s or until it would strike something. 5-round clips (10 in reserve, 2.5 s reload). It has no spread, and the game's mortar reticle (`mc_crosshairMortar`). It doesn't kick the view (its camera recoil is zero), only jolts it (its `CameraShakes`: intensity 0.15 with a roll, over a fifth of a second). It takes three quarters of a second to come out (its `EquipTime`). It has its own mesh and textures, its firing animation (the gun's kick with the arm's light recoil at twice its speed), idle, reload, equip, holster and change-hands animations, its sounds (shot, reload, equip, change hands, the round's flight and its splash; with nothing left to fire, the dry click with the grenade launcher's dry detonation), its HUD icon and its effects, the game's own particle systems (below): a burst of slime at the muzzle with a white light; the round flying as a glob of slime, with its own light; a splash of slime where it goes off; dark fumes drifting off the gun while it is ready; and steam as it reloads. (In the game the slime also slows the enemies it hurts for 3 s, `TnBuffAoeSlow`, never the player's own side; the demo's enemies hover in place, so it has nothing to slow.) Online, its rounds fly and go off on the other players' PCs too
- **The Gear Shredder**, the Scientist's weapon (a Scientist build with it in its loadout starts the match with it): the game's saw-blade launcher (`TnWeaponSawblade`, the "SawBlade" inside; `SawBlade_SV_WEPDATA`, which Escalation uses, firing `TnProjectileSawblade` with its campaign data, `SawBlade_SP_PROJDATA`, as Escalation does: the blade has no Escalation data of its own). **Hold the trigger and it loads saw blades onto the gun**, one at once and another every 0.65 s, up to three and no more than its clip holds, each spinning on its own muzzle, with the charge's hum and a clank as each one loads; **let go and they all fly**, a round from the clip for each, the second and third 5 degrees out to the right and left, from the gun's three muzzles. A pull in the half second after a shot, before it is ready again, is lost, as in the game. Each blade flies at 140 m/s in a straight line; **for its first 1.25 s it bounces off whatever it strikes**, keeping 0.85 of its speed each time (up to ten bounces), and it bounces off the other players at any time but passes through the one who fired it; **once it has bounced it homes on the enemy nearest its line within 10 m**, turning hard for it; after 1.25 s it sticks in the first thing it strikes. An enemy it sticks in takes 165 damage and keeps the blade until it is destroyed; a blade stuck in the level stays there 10 s. While it charges Jazz moves at 0.85 of his speed and can't sprint; a reload or changing weapon drops the blades loaded. **Fine aim shows each blade's path** (one for each blade loaded, one at least), bounces and homing and all, simulated afresh every frame from its muzzle, a twentieth of a second at a time for a second or until it would stick in an enemy: two lines fading out along the way, bright blue while the blade would miss and red once it would strike an enemy (the colours the game's data gives it: below). 6-round clips (12 in reserve, 2 s reload). It has no spread, and the game's saw-blade reticle (`mc_crosshairSawblade`). Each shot widens the view by 1.5 degrees (1 in fine aim), easing back over 0.7 s, and jolts it hard (its `CameraShakes`: intensity 0.5 with a roll, over half a second). It comes out in a fifth of a second (its `EquipTime`). It has its own mesh and textures, its animations (its charging loop, its shot's kick and its charged shot's bigger one, each with the arm's recoil, idle, reload, equip, holster and change hands), its sounds (the charge's hum and the blades loading, the shot by how many blades, the reload; the Neutron Assault Rifle's equip, holster, change-hands and sights cues, as the game gives it; the blades' whirr, bounce and impact), its HUD icon and its effects, the game's own particle systems (see *The Scientist's Gear Shredder*, below): each blade spinning and glowing on the gun as it loads; each flying blade a spinning, glowing saw blade with streaks off its edges and a white light; an electric burst where it sticks, its glow warm on the Autobots' maps and blue on the Decepticons' (the side that fired it); the blade left stuck there; and steam as it reloads. Online, the other players see the blades load onto his gun, and his blades fly, bounce and stick on their PCs too
- **The Thermo Rocket Cannon**, one of the Destroyer's weapons (a Destroyer build with it in its loadout starts the match with it: Choose Character, above). It is the game's homing rocket launcher (`TnWeaponRocketLauncher`, the "HomingRocket" inside; `WEP_RocketLauncher_p`'s `RocketLauncher_SV_WEPDATA`, which Escalation uses, firing `TnProjectileHomingRocket` with its campaign data, `RocketLauncher_SP_PROJDATA`, as Escalation does). **Keep the crosshair on an enemy for 0.75 s and it locks on**: the game's lock-on marker closes in on the enemy as the lock comes (its three prongs drawing together and brightening) and settles onto it as it locks, and the lock tone sounds; the lock holds for 2 s after the enemy leaves the crosshair, and is looked for only while the gun is ready with a rocket in it. Each pull of the trigger fires a rocket at 70 m/s: fired locked on, it homes in on that enemy (steering at 400 m/s² and, within 20 m of it, at 600 m/s², going off a second after it began closing in if it hasn't struck it); fired without a lock, it flies straight. It goes off on the first thing in its way (a wall, the floor, an enemy, another player) for 375 damage to every enemy within 7 m (full out to 2.5 m, half beyond), through walls, and a quarter of that to Jazz if he is that close. Its clip holds one, reloading at once (2 s; 5 in reserve). Its spread opens from 0.04 by 0.02 a shot to 0.08, settling back over 3 s (half that in fine aim), with the game's rocket reticle. Each shot kicks the view up a quarter of a degree and jolts it (its `CameraShakes`: intensity 0.6 with a roll, over half a second), and **its blast shakes the view** of a player near it but outside its reach (out to 150 m, less the further away), as the game's projectile camera shakes do. Jazz holds it in the heavy-gun stance; it comes out in a fifth of a second. It has its own mesh and textures, its firing animation (the gun's kick with the arm's heavy recoil), idle, reload, equip, holster and change-hands animations, its sounds (shot, lock tone, sights, reload, equip, holster, change hands, the ballistic dry fire, the rocket's roar in flight and its blast), its HUD icon and its effects, the game's own particle systems (see *The Destroyer's weapons*, below): a fiery flash at the muzzle and a burst of flame out of its back with every shot; the rocket itself flying with its fire and smoke trail and a white light; a rolling fireball where it goes off; dark smoke and steam as it reloads. Online, its rockets fly, home and go off on the other players' PCs too
- **The A-4 Pulsar Cannon**, another of the Destroyer's weapons: the game's sticky plasma-grenade launcher (`TnWeaponGrenadeLauncher`, the "GrenadeLauncher" inside; `GrenadeLauncher_WEPDATA`, the data a match gives it: Escalation has none of its own for it, firing `TnProjectileGrenadeLauncher` with its `GrenadeLauncher_SP_PROJDATA`). Each pull of the trigger fires a ball of plasma (a second apart at the quickest) at 45 m/s, near enough straight, that **sticks in the first thing it strikes** (a wall, the floor, an enemy, another player: on the bone of his nearest where it struck, riding it as he moves, as the game's `AttachToActor` does) and goes off 2 s later, facing back the way it came: 225 damage to every enemy within 7.5 m (full out to 3.5 m, half beyond), through walls, and a quarter of that to Jazz if he is that close. **Press the right mouse button to set off every round you have out** at once, with the game's detonation sound (with none out, its dry click); while you have some out the HUD shows the game's DETONATE prompt under the crosshair with that button. Three can be out at a time: a fourth sets off the oldest; an enemy carrying one sets it off as it is destroyed, and picking up another gun in its place sets them all off. Each blast **shakes the view** of a player near it (out to 300 m). 3-round clips (9 in reserve, 2.5 s reload). Its spread is a steady 0.01, with the game's cannon reticle; it has no fine aim (that button sets its rounds off). It doesn't kick the view (its camera recoil is zero), only jolts it (intensity 0.7 with a roll, over 0.4 s), and takes three quarters of a second to come out. It has its own mesh and textures, its firing animation (the gun's kick with the arm's light recoil), idle, reload, equip, holster and change-hands animations, its sounds (shot, the round's hum in flight, its fizz once it sticks, its blast, the detonation and its dry click, reload, equip, holster, change hands, the ballistic dry fire), its HUD icon and its effects (below): a violet flash at the muzzle with a violet light; a ball of plasma glowing on the gun while it is ready; each round a crackling ball of plasma with a trail, its glow warm on the Autobots' maps and blue on the Decepticons'; an electric blast where it goes off; and steam as it reloads. Online, its rounds fly, stick and go off on the other players' PCs, and his detonations set them off there
- **The Chaos-Rift Combustor**, the last of the Destroyer's weapons: the game's cluster-bomb mortar (`TnWeaponClusterCannon`, the "ClusterCannon" inside, on the grenade arm's mesh; `ClusterCannon_SV_WEPDATA`, firing `TnProjectileClusterCannon` with its `ClusterCannon_SV_PROJDATA`, its clusters `TnProjectileClusterCannonBomb` with `Clusters_SP_PROJDATA`). Each pull of the trigger **lobs a bomb at what the crosshair is on** (out to 50 m; further, 50 m along the aim): the game works out the throw that lands it there in 0.1 to 1.1 s by the distance, and it falls under the world's gravity, going off on whatever it strikes for 150 damage to every enemy within 4.5 m, and **bursting into five clusters** thrown out of the surface it struck at 15 to 25 m/s. **Pull the trigger again while the bomb is in the air and it bursts there**, its clusters carried on along its path; the HUD shows the game's CLUSTERBOMB prompt with the fire button while it flies. The clusters fall and bounce, each going off 0.5 to 1.75 s after it first lands, or at once on an enemy or a player it strikes, for 150 damage to every enemy within 8.5 m (every cluster counts), through walls, and a quarter of that to Jazz if he is that close; each **shakes the view** of a player near it (out to 150 m). **Hold the right mouse button to see where it would land**: the game's arc along the lob, simulated afresh every frame, and its impact marker, a yellow glow on the ground where the bomb would come down. Its clip holds one, reloading after each shot (2.5 s; 5 in reserve). Its spread opens from nothing by 0.04 a shot to 0.04, settling back over 2 s, with the game's mortar reticle. It doesn't kick the view, only jolts it (intensity 0.3 with a roll, over 0.8 s), and comes out in a fifth of a second. It has its own mesh and textures, its firing animation (the gun's kick with the arm's light recoil at twice its speed), idle, reload, equip, holster and change-hands animations, its sounds (shot, the bomb's flight and blast, the clusters' whistle, bounce, fuse and blasts, reload, equip, holster, change hands, the plasma dry fire), its HUD icon and its effects (below): a flash with rippling rings at the muzzle and a glowing spent shell flung out with every shot; the bomb, the game's mine ball trailing dark smoke with a bright white light; its blast; the clusters, the game's canisters, and theirs; and steam as it reloads. Online, its bombs, their bursts and their clusters play out on the other players' PCs too
- **The Neutron Assault Rifle** on the floor too, off to Jazz's right as a match starts (5 m to his right and a step ahead, across from the Subsonic Repeater, side on with the same cyan outline; hold E there to pick it up in place of the gun in his hand). It is a Scientist's weapon in the game: a Scientist's build with it in its loadout starts with it, and for the others it waits there. (Without the Choose Character screen's files, Jazz starts with it, as before)
- **Escalation's enemies** (`src\enemies.rs`, built by `tools\build_enemies.py`): the game's own enemy types for each map, brought in by its waves (next); the soldiers fight as the game's do (*The enemies' AI*, below), the other types don't move or fight yet. Each is built as the game builds it: its model with a part picked for each of its mesh set's slots (the soldiers' backpacks, wheels and wings), painted in one of its colour pairs with its energon glow, playing its idle. Shots, rounds, blades, blasts and melee strike its physics asset's bodies where its bones are (its head's for a headshot), and its health and armour are its behaviour's. It dies as its death set has it for how it was killed: a soldier shot from the front plays one of its gunfire deaths, a headshot its own, an electrocution or the slime's stagger theirs, and the rest fall at once; then it falls as a ragdoll on its physics asset, and once it has come to rest (and lain as long as the death says) it dissolves from its middle in the game's cyan, with the death set's lights and sounds. (The test enemies that stood near every map's start, coming back 10 s after they died, are gone.)
- **Escalation's waves** (`src\waves.rs`, from each map's own data, `tools\build_waves.py`): the game's 15 waves on every map, run as the game runs them. The countdown to each wave shows on the HUD's clock, with the announcer (Optimus Prime, or Megatron on the Decepticon maps) counting down "Five" to "One". Each wave brings its own enemy types from the map's spawn list, one at a time, at the map's spawn points out of the players' sight (none behind a locked door), with "WAVE 1 OF 15" on the HUD. The last three get markers through walls, with "We are 3 kills from victory!" and "We are 1 kill from victory!". A wave won pays every player its reward and brings the dead back; a lost wave costs a life and starts again; the game is won after the 15th. See *Escalation's waves*
- **The enemies' AI** (`src\ai\`, from the game's own planning domains, behaviour values, animations and sounds: `tools\build_ai.py`, `build_ai_anims.py`, `build_nav.py`): Fall of Cybertron's HTN AI ported from its script, for Escalation's soldiers (the Autobot soldiers of Downfall and Oblivion, the Decepticon soldiers of Ancients and Ignition), Scrappers, Shotgunners, Rocket Troopers, Heavy Gunners, Sharpshooters, the Guardian, the Leaper, Ancients' Swarmers and the Snipers. Each plans with the game's planner on its type's domain, senses the players (sight, hearing, pain, its squad's word), finds its way on the map's navigation mesh, takes the cover the game's tactical system picks, fires its gun as its competency allows, melees players who come close and throws grenades (one at most every 30 s in a squad), plays the game's animations through its anim tree (moving in every direction, turning, aiming, peeking and firing from cover, jumping, reloading, reacting to hits) with their effects, and talks: footsteps, its animations' sounds and its lines through the game's dialog manager. Each type fights its own way: the Shotgunners dash back firing and step aside, the Rocket Troopers swap rifle and launcher, the Sharpshooters cloak and lock on before they shoot, the Scrappers take turns at a player hand to hand, the Heavy Gunners drag their fire across him, and the Guardian searches in its investigate form, sweeping its scanner beam (which strips a player's cloak off and jams his ability), then stands up into its battle form with its charged Guardian Blaster and a blast that throws the players round it back, and the Leaper leaps 25 m up across Ignition's arena and dives onto a player, lunges and skids at him nearer, and blasts him away with its booster vents when he keeps close, and the Swarmers scuttle at a player as a pack and bite, and the Snipers fly from perch to perch in their robot form and fight from the walls in their insect form, their laser sights locking on before they shoot. Its grenades bounce off the level and the players, go off 4 s after they first strike something, and show on the HUD of a player within 18 m of them (the game's grenade marker, kept at the screen's edge with an arrow when it's off the screen); a player's grenade that bounces to a near stop within 20 m of soldiers sends them diving away. See *The enemies' AI*.
- **Energon from kills**, Escalation's money: as in the game a match starts with 250, and each kill pays the enemy type's Escalation score (`TR_Survival_m.SurvivalScoring_p`'s `AiScores`: a soldier 40) times the difficulty's multiplier (0.75, 1 or 1.25), split as the game splits it (`TnSurvivalScoring`): the killer is paid for the health the others didn't take, each player who helped for what he took, and a kill nobody made pays half to each. The HUD's energon counter counts it up with the "+*N*" the game shows. (The energon cubes are gone from the maps: Escalation pays for kills instead.)
- **Health stations**, where each map has them (five on Downfall and Oblivion, three on Ancients, four on Ignition: the game's Escalation health pickup factories): the game's health pickup base with an energon cube hovering and turning over it. Stand near one with health or shield to make up and the HUD offers "Hold E to purchase Health for \*200" (the game's prompt, the price in its energon numbers); **hold E** to buy a full repair, health and shield (the game's 1000-point health pickup, healing health first and then the shield) for 200 energon, with the game's pickup sound. The cube is then gone for 40 seconds (the station's respawn time). Without enough energon you get the game's error beep instead
- **Ammo stations**, where each map has them (six on Downfall and Oblivion, three on Ancients, four on Ignition: the game's Escalation ammo pickup factories, on the bases the map has for them): the game's ammo crate (`AmmoPickupLight_STAT`) hovering a metre over the base and turning (at the game's rate, a full turn every 6.5 seconds), in its own textures with its cyan-glowing panels and the glowing cyan outline the game draws around pickups. Stand near one with any of your guns short of a full reserve (your two weapons or the car's guns) and the HUD offers "Hold E to purchase Ammo for \*100" (the game's prompt and price); **hold E** to buy it for 100 energon, with the game's ammo pickup sound: as in the game, every gun you carry gets its reserve filled to the top, the clips as they were, except that an empty clip reloads (the gun in your hand with its reload, the others at once), and the HUD lists what your weapons got over the ammo count, each with the weapon's small icon (the game's `mc_ammoAddedNotification`, with the kill feed's `GameMessageIcon` of the weapon; the vehicle's own for its gun). The crate is then gone for the station's respawn time (10 seconds on Downfall, 20 on Oblivion, Ancients and Ignition, as each map sets it). Without enough energon you get the error beep instead. The game's heavy-ammo stations, which only serve the heavy weapons the demo doesn't have, are left as their bare bases
- **Four of the game's Escalation maps**, picked in the Escalation lobby with **Select Map**:
  - **Downfall** (`ESC_Iacon`, a street of Iacon at sunset), imported whole from the game: its floors and walls (the level's BSP geometry, read straight from the map package) and its 2,200 placed meshes with every texture at the game's full size; every area open, as the level stands once each console has been bought (the ordnance doors slid away into the walls, the start area's bars dropped into the floor and ceiling, the retractable cover folded away); the Escalation consoles with their keyboards and animated screens, the loot pods, and the big Optimus doors at the streets' ends standing open on their electric barriers; its sunset sky seen through the level's haze, softened a little by its depth of field, with the sun's light shafts; collision for walking, jumping, stairs and walls, with its invisible walls; and its own player start, 112 enemy spawn points, five health stations and six ammo stations
  - **Oblivion** (`ESC_Ark`, aboard the Ark), imported whole the same way: its BSP and its 4,200 placed meshes with every texture at the game's full size, block-compressed with mip maps as the game keeps them; its transforming doors, posed as the level starts them; its windows' glass and the space outside them (the star fields and nebula of its sky dome, Cybertron, the space bridge and the vortex), drawn with the game's own materials and moving as the game's do, past 500 m softened a little by the level's depth of field; its light maps, lights, haze, bloom and colour grade; collision with its invisible walls; and its own player start, 77 enemy spawn points, five health stations and six ammo stations
  - **Ancients** (`ESC_Temple`, a rusting canyon strewn with the wrecks of giant Guardian robots), imported whole the same way: its BSP, its terrain (the canyon floor, its pits and cliffs, which umodel can't export: read from the map package, smooth as the game draws it, its rust, plain-rust and cliff-rock layers painted over each other as the game's weight maps paint them, with its own light maps and collision) and its 1,166 placed meshes with every texture at the game's full size; every area open, as the level stands once each console has been bought (the bars of pillars dropped into the floor and ceiling, the dropship doors lifted away, the sliding doors slid aside, the big Optimus doors the consoles open standing open, the retractable cover folded away); the toxic sludge glowing in its pits, the floor debris's painted shadows and the faint orange force fields at its edges; its grey sky dome under the level's bright distant haze, and the sun's warm glare; its light maps, lights, haze, bloom and colour grade; collision with its invisible walls; and its own player start, 88 enemy spawn points, three health stations and three ammo stations
  - **Ignition** (`ESC_Gladiator`, a Decepticon arena in Kaon), imported whole the same way: its BSP and its 2,756 placed meshes with every texture at the game's full size; every area open, as the level stands once each console has been bought (the green force-field gates lowered into the floor or raised into the ceiling, all 24 big Optimus doors standing open); its consoles, loot pods and steam vents, posed as the level leaves them; its gold statue of Starscream, shining and reflecting the arena; the green and blue light bars and the other glows as its materials make them; the faint green force fields at its edges; its sky (a sunset dome with stars, banks of bright cloud and star fields, lost in the level's dark green haze) and its two moons showing through it, pale in their bluish halo, all of it softened by the level's depth of field, as the game shows them; its light maps, lights, haze, bloom, depth of field and colour grade; collision with its invisible walls; and its own player start, 130 enemy spawn points, four health stations and four ammo stations
  - On each, the waves bring the enemies in at the map's own Escalation spawn points, and shots and grenades stop at walls
- **Changing form beside a wall:** a vehicle that comes into being overlapping a wall
  (a jet or car unfolding with its back to an enemy spawn door) is moved out on the
  side of the wall the player stood on, as the game finds it room
  (`TnPawn.MoveToSafeTransformationLocation`), never pushed on through a thin wall
  by the parts of it already past it
- **The maps are lit like the game:** Downfall with its own baked lighting (its Lightmass light maps, read from `ESC_Iacon_ART_m_LM.xxx` and `ESC_Iacon_BASE_m_LM.xxx`, plus the vertex lighting of the props that use it), the level's low pink sun with real-time shadows, its 81 point lights lighting Jazz and the enemies, and the level's post-processing: its three layers of haze (lavender, peach and rust), bloom and its colour grade (`VortexFlyingPrevis_CLUT`); Oblivion the same way, from `ESC_Ark_ART_m_LM.xxx`, with its 136 lights, its warm haze, bloom and colour grade (`ENV_ARK_BridgeWarmUpdated`; it has no sun: it is all indoors); Ancients from `ESC_Temple_ART_m_LM.xxx` (its terrain's light maps too), with its high, warm sun, its 58 lights, its pale haze, bloom and colour grade (`templeCLUT`); Ignition from `ESC_Gladiator_ART_m_LM.xxx`, with its pale sun, its 236 lights, its dense green haze, bloom and colour grade (`L05_Soundwave_CLUT`). The bloom is the game's: a small glow round whatever is brighter than white (UE3's 16-pixel bloom kernel at its 1280-wide reference), at each map's strength (Oblivion's the strongest, 0.2), never a haze over the whole screen. Ignition, Downfall and Oblivion have the game's depth of field too, as their post-process volumes turn it on (Ancients' doesn't): the far distance softened, never what is near - Ignition's sky, moons and far spires (blurred from about 100 m out, 0.8 of the way from 190 m on, with a 32-pixel kernel), Downfall's sky (from a kilometre out, half at 2 km) and Oblivion's space outside its windows (past 500 m, up to 0.4); see "The maps' depth of field". Their glowing parts glow as each material sets them (its glow colour and strength)
- **The maps' effects from the game:** the light shafts under Ignition's lamps, the glow spheres, smoke and fog cards and pin lights, the burning smoke columns over Downfall, the steam vents, fires and rust storm, and the lens flares on the lights, all drawn with the game's own materials (compiled from their material graphs into a shader) and textures, soft where they meet walls and floors. The particles are the game's own particle systems, run frame by frame as its engine runs them (see "The maps' effects")
- **Sound effects from the game:** each gun's shots, end-of-burst tail, equip and holster sounds and its bullet impacts; the car's guns, engine (pitched by speed), boost, tyre squeal, crashes, jumps, landings and destruction, each armor set's vehicle with its own (a jet's hover, boosters, climbs, dives and flight too); a tank's cannon, a jet's rockets and lock tone and a truck's rockets; Jazz's footsteps, sprint start, jumps, dashes, landings, transformation (each armor set's own), melee swings, grenade throw and blast, hit reactions and death, and his shield running out and recharging; enemy shots and explosions; energon pickups; buying health at a health station and ammo at an ammo station, and the error beep when you can't afford them; buying armor on the Customize screen, and its error sound when you can't; the countdown's tick on the starting scoreboard; and, as the match begins, what the game plays then: Optimus Prime announcing "Escalation" and the match-start music (one of the game's 13 stingers from the campaign's score, at random). They play the way the game's sound cues describe (layered recordings with timing and random pitch/volume variation, volume curves), fade with distance as the game fades them, no more copies of each at once than the game allows, and the sounds tied to animations fire at the same moments as in the game
- **Firing effects from the game:** every gun's muzzle flash, and the car's guns', is the game's own particle system, with its flash of light; tracers, impact bursts (flash, fire spears, embers and smoke) and spent shells for the rifle, the Subsonic Repeater, the Photon Burst Rifle, the Scatter Blaster, the Nucleon Charge Rifle, the Path Blaster, the car's guns and the attack drones, built from the game's own effect textures and the timings, sizes and colours of its particle systems; the Riot Cannon's muzzle flash, flying round and explosion, the Orbital Pistol's muzzle flash, beam, impact and steam, the X18 Scrapmaker's, the Energon Harvester's, the E.D.K. TechVolt's and the Corrosive Slime Cannon's effects (their flashes, tracers, beams and chains, impacts, rounds, smoke, shells' trails, heal, idle effects and steam), the Gear Shredder's, the Thermo Rocket Cannon's, the A-4 Pulsar Cannon's and the Chaos-Rift Combustor's (their flashes, blades, rockets, plasma rounds, bombs and clusters, their trails, blasts and steam), and a tank's shells', a jet's rockets' and a truck's rockets' muzzle flashes, flight and blasts are the game's particle systems themselves, run as its engine runs them. The rifle's, the Subsonic Repeater's and the car guns' rounds leave glowing bullet holes on the walls (the Subsonic Repeater's glowing cyan), the Path Blaster's scorch marks, and all the guns but the Photon Burst Rifle steam as they reload (below)
- **The game's HUD**, played from Fall of Cybertron's own HUD movie (`UI_GFxHud_p.Hud_GFX`): the shield and health bars and the health number (the health bar flashing white on every hit, the shield bar as it starts to recharge; both red, with the game's tunnel-vision and red-scanline low-health effect, when the shield is down and health is under three quarters), the weapon's name and wireframe icon, the ammo bar (a segment per round for the rifle) and round count with low-ammo and empty warnings and the RELOAD prompt, the rifle reticle that opens with spread and turns red on a target (each gun's own: the tactical, shotgun, pistol and cannon ones too), the scopes (the Photon Burst Rifle's tactical scope, the Nucleon Charge Rifle's sniper scope with its charge), the reload ring, hit pings, damage arrows on a ring around the crosshair, the frag-grenade slot, the energon counter with its "+N" and "-N" pop-ups (a change while one shows added to it), ammo-pickup notes, and "ESCALATION" announced across the top as the match begins (typed out letter by letter behind a cursor, held for 5 seconds, then wiped away, as the game shows its game announcements). It is laid out as on PC, with its corner groups at the window's edges (the PC version's default screen safe frame of 0; the consoles keep them 7.5% in), and drawn in 3D the way the game draws it, with the corners turned in towards you (more while fine aiming, flattened in the car)
- **Ammo and reloading** with the game's numbers: rifle 36-round clips (216 in reserve, 1.5 s reload), Subsonic Repeater 60 (300, 1.25 s), Photon Burst Rifle 15 (105, 2 s), Scatter Blaster 8 (40, 2.5 s), Nucleon Charge Rifle 4 (20, 2 s), Path Blaster 10 (40, 2.5 s), Riot Cannon 4 (24, 2.5 s), Orbital Pistol 6 (24, 1.25 s), X18 Scrapmaker 200 (none in reserve), Energon Harvester 6 (24, 2.5 s), E.D.K. TechVolt 50 (200, 2.5 s), Corrosive Slime Cannon 5 (10, 2.5 s), Gear Shredder 6 (12, 2 s), Thermo Rocket Cannon 1 (5, 2 s), A-4 Pulsar Cannon 3 (9, 2.5 s), Chaos-Rift Combustor 1 (5, 2.5 s), car guns 100 (400, 2 s), a tank's cannon 1 (5, 2.25 s), a jet's rockets 4 (16, 3 s), a truck's rockets 8 (8, 1.5 s). An empty clip reloads by itself a fire interval after its last round, the trigger held or not, as the game's refire check does it (a burst gun after its wait between bursts), **R** reloads early, with the guns' own reload animations and sounds; changing form reloads every weapon you carry, your two guns and your vehicle's weapon, at once and without a sound, a second into the change (when the game brings out the new form's weapon: `TnPawn`'s `TransformToAltShootingDelay` and `TransformToRobotShootingDelay`, and `TnInventoryManager.ReloadAllWeapons`), each clip as full as its reserve allows; the ammo stations (below) fill them up. Shots spread like the game's: the rifle opens up as you hold the trigger and settles back, and fine aim tightens it
- If no map has been built, an arena built from the game's own walls, vent grates, pipe runs and piston towers is used instead, under the Iacon sunset sky dome
- **The game's Escalation lobby**, rebuilt from its lobby movie (`UI_GFxLobbies_p.MPLobby_GFX`) with its player list and prompts: the drifting teal clouds, the panels, ESCALATION with its ticker, the map's picture in its frame, the game's menus with their selection bar, hints and difficulty selector, the player list under the map's team (AUTOBOTS, or DECEPTICONS in purple on the Decepticon maps, Downfall and Oblivion, as in the game) with names, scores and connection bars, "Waiting for host to start game", the MATCH STARTS IN countdown, its sounds. Its party lobby has **Offline Game** (a game of your own, alone or split screen, with no network and no password), **Join Game**, **Host a Game** (with an optional **Game Password**: only players who type it can join) and **Display Name** (in Leaderboards' place: it opens a prompt to type the name the other players see, in the lobby's player list and over your Jazz in a match; the game's Friends List is left out); a game's lobby has **Start Game** and **Select Map** (for the host: Downfall, Oblivion, Ancients or Ignition, with each map's picture and name from the game; those who join see the host's choice), Display Name, **Customize** (the Customize screen, closing back to the lobby), the **Difficulty Setting** (Easy, Normal or Hard: how hard the enemies hit, and how hard you hit them) and Settings
- **Online co-op for up to four players:** one player hosts, the others join with the host's IP address, and the host starts the match for everyone. Each player plays his own Jazz and sees the others' running, jumping, transforming, driving, aiming, shooting (with their muzzle flashes, tracers and impacts), throwing grenades and meleeing, with their sounds, as their PCs play them. Enemies, the weapons on the ground, the health and ammo stations and the energon counter are still each PC's own for now: the others' shots and grenades do damage on your PC, so the same enemies tend to go down everywhere, but each PC keeps its own energon, and a weapon one player picks up is still lying there for the others (as are the health and ammo at a station he buys from)
- **Every Jazz runs on his own**, each with his own spawn point around the map's start. A shot hits whichever Jazz is in its way, and the other players' sounds come from where they are

These are the game's **Default** keyboard layout; SETTINGS > Controls >
Keyboard/Mouse Layout picks another of the game's layouts or changes any key.

| Key | Action |
|---|---|
| Click window | Capture the mouse |
| Mouse | Look (as fast as Mouse Sensitivity says; Robot / Vehicle / Jet Camera Inverted turn its up and down round). Boosting a car or truck on its wheels, moving it left and right steers (the faster, the harder; the view follows the car round) and up and down still looks. Flying a jet, it steers: the view turns the way you move it, at the flying camera's rates |
| W A S D or the arrow keys | Move (robot). In the car: strafe the hovering car; it faces where you look with the mouse (a hovering jet strafes in the view's frame, up and down with it too) |
| Shift | Boost dash (tap; robot, in the direction you're moving, once per jump in the air) / sprint (hold: no dash; forward, or forward and to a side) |
| Space | Jump. In a hovering jet: climb |
| Control | In a hovering jet: dive (Descend) |
| G | Throw a grenade (every 2.5 s) |
| Q (hold, then let go) | EMP Grenade, for a character that has it: hold to see where it will go, let go to throw |
| Middle click | Melee: your class's sword swing (with the Ion Blade for the Infiltrator and the Scientist), or a charging punch while sprinting forward |
| F | Transform (on the ground or in mid-air) |
| Left click | Fire (a tank's cannon and a jet's homing rockets: one each pull; a truck's rockets: bursts of two while held) |
| Right click (hold) | Fine aim: zoom in and aim down the sights. In the car, boost: down onto the wheels, flat out, steered with the mouse (with Toggle Vehicle Boost, press to start and stop); a tank boosts straight ahead, a jet flies |
| C | Switch the gun to the other hand; the camera moves over to that shoulder |
| R | Reload |
| Mouse wheel | Swap between the two weapons carried (at the start: the chosen character's two) |
| E (hold) | Pick up the weapon in front of you, in place of the one in your hand; at a health or ammo station, buy health or ammo |
| Esc (or a gamepad's Start) | Pause menu (the game keeps running) |
| Tab | The scoreboard; Tab again (or Esc) puts it away |

The layouts also have the campaign's Assault and Utility T.E.C.H. (1 and 2), which do
nothing in the demo.

In the menu: the mouse, the arrow keys or W/S with Enter, or a gamepad (d-pad or
stick, A to choose, B to go back). In SETTINGS, Enter or a click opens Graphics,
Audio or Controls; there Left/Right or Enter steps a setting through its values
(or click one), and Left/Right (or click or drag the slider) sets a slider. In
Controls, Keyboard/Mouse Layout opens the keyboard layouts: Select Layout and
View Layout step the same way, and Down goes on into the table of keys, where
Enter or a click on a row waits for its new key: press it, click a mouse button
or turn the wheel (Move takes four keys in turn: forward, left, back and right;
Esc stops waiting). In EXTRAS, Enter or a click on Credits plays the ending credits;
Esc, Enter, Space, a click or a gamepad button skips them. Esc, a right-click or
the Back button goes back a menu. In the Escalation lobby, Up/Down (or the mouse)
move through the rows that can be used, Enter or a click chooses, and Left/Right
or the arrows change the difficulty; in Join Game, type the host's address and
press Enter (Backspace rubs out, Esc cancels), and the same for a Game Password.
**Ctrl+V** (or Shift+Insert) pastes what you have copied into any of these
prompts: the address, a Game Password, Display Name and a custom build's name.

In CUSTOMIZE: Left/Right (or A/D) choose a class and Up/Down (or W/S) one of its
three builds, Enter, Space or a click opens it in the editor, and Esc, Backspace or a
right-click goes back (from the class select, back to the main menu, or to the
Escalation lobby it was opened from). In the
editor, Left/Right or Tab turn the stack of pages, Up/Down move through a page's
rows and Enter opens one; in a list, Up/Down (PageUp/PageDown, Home/End, the mouse
wheel) choose, Enter accepts and Esc goes back to what was there before. On the
Metallic slider, Left/Right (or drag it) set it. R (or Y) randomizes the build, C
(or F) changes form, L unlocks the camera, after which Q/E or dragging with the
mouse turn round the robot. The hint buttons at the bottom do the same with a
click. With a gamepad: d-pad, A to choose, B to go back, X Toggle Faction, Y
Randomize, the left stick's button Change Form, d-pad right (or the right trigger)
Cycle Page, and the right stick's button unlocks the camera, after which the right
stick turns it; while you use the gamepad, the hint buttons show these buttons'
pictures.

In the game, Esc opens the pause menu: RESUME GAME (or Esc again) goes back to the
fight, CHOOSE CHARACTER opens the Choose Character screen, SETTINGS opens the same settings menus as the main menu, and QUIT GAME
asks whether to quit (the match's experience is lost) and, on Yes, leaves the match
and goes back to the main menu (a host leaving ends the match for everyone; the
next match starts afresh, with Jazz at the start, healed and rearmed and the energon
back to 250). The host also has RETURN TO LOBBY: on Yes the match ends and every
player goes back to the game's lobby, still in the game (the others are told the
host ended the match), and the experience earned is kept. In a question, the
orange box shows the button in focus (Yes as it opens): Left/Right (or the d-pad,
or the mouse) move it, and Enter, Space, A or a click presses it; Esc, Backspace,
B or a right-click answers No.

On the Choose Character screen (as a match starts, and from the pause menu): Up/Down
(or W/S, the d-pad or the left stick) or the mouse move through the builds that can
be played, and Enter, Space, a click or A chooses the one in focus. From the pause
menu, Esc, Backspace, a right-click or B goes back to it without choosing; as a
match starts there is no going back, as in the game. The screen has no hint
buttons, as the game's has none on PC. After it, the starting scoreboard counts
down to the match; there is nothing to do but wait (Esc opens the pause menu).

With split screen (below), Player 2 plays on his own controller, as the first
player plays on his with Active Game Controls on Controller (the controller layout
chosen in Controls is his too), and Start on it opens the pause menu for him. In a
game's lobby you host, Start on a second controller brings him in, and B on it
takes him out; the menus don't take his controller.

---

## 1. Install Rust (one time, Windows)

1. Go to https://rustup.rs and run `rustup-init.exe`.
2. If it says you need the **Visual Studio C++ Build Tools**, let it install
   them (choose "Desktop development with C++"). This is required.
3. Close and reopen your terminal, then check it worked:
   ```
   cargo --version
   ```

## 2. Run the demo

Unzip this folder somewhere, e.g. `Documents\foc_esc`, then in a terminal:

```
cd Documents\foc_esc
cargo run --release
```

(`boot demo cargo.bat` does the same.) The **first build takes about eight
minutes** (it compiles the whole engine, fully optimized, and the H.264 decoder
for the menu's background movie with the C++ Build Tools). Later runs start in
seconds, and after an update the demo's own code rebuilds in about two minutes.
`cargo run` without `--release` (`boot demo cargo - dev build.bat`) builds the
debug build instead: its own first build is quicker, but it runs big waves more
slowly (the demo's own code is less optimized there). The main menu comes up first: choose ESCALATION, then Host
a Game and Start Game to play (alone, or with friends who join you: see "The
Escalation lobby and online play" below), and click inside the window to capture
the mouse if it isn't already.

### The program's icon and name

`foc_esc.exe` carries its own name, **Cybertron Escalation Standalone**: its version
information (File description and Product name, which File Explorer lists under the
file's Properties > Details) says so, and `build.rs` puts it in as the program is
built. Task Manager names the program by it, and so do recording tools that name
their folders after the program: NVIDIA's (the NVIDIA app's and GeForce
Experience's) put its videos and screenshots in a `Cybertron Escalation Standalone`
folder. Without a name of its own, NVIDIA's named that folder after the one the
program is in, `release`.

`foc_esc.exe` (in `target\release`, or `target\debug` for the debug build) also carries the demo's icon, `icon\foc_esc.ico`:
File Explorer shows it for the program, and the game puts it on its window and on
the taskbar as it opens (`src\appicon.rs`). `build.rs` puts it into the program
as it is built, with nothing extra to install. To use another picture, make the
icon from it (a PNG, best square with a transparent background) and build again:

```
python tools\make_icon.py my_picture.png
cargo run
```

(`icon\foc_esc.png` is the picture the current icon was made from. File Explorer
sometimes keeps showing a program's old icon for a while after it changes.)

## 3. Build the assets from your game (one command)

Everything the demo shows and plays from the game (the maps, characters, guns,
effects, sounds, HUD and menus: the `assets` folder but for its shaders and
settings files) is built from your own copy of Transformers: Fall of Cybertron by
the scripts in `tools`. `tools\extract_game.py` runs them all, in order, with the
UE Viewer exports each one needs:

1. Install **Python 3** (3.9 or newer, from https://www.python.org; tick "Add
   python.exe to PATH"; the scripts were tested with 3.13) and its three packages:
   ```
   pip install numpy pillow fonttools
   ```
2. Install **ffmpeg** (https://ffmpeg.org/download.html, e.g. the "essentials"
   build) and put its `bin` folder on the PATH: the sounds and the menu's movies
   need it.
3. Download **UE Viewer** for Windows (https://www.gildor.org/en/projects/umodel)
   and unzip `umodel.exe` and `umodel_64.exe` (with their `SDL2` files) into
   `tools\umodel`.
4. Double-click **`extract assets.bat`** (or, in a terminal in the demo's folder,
   `python tools\extract_game.py`).

It finds the game in Steam's libraries (those Steam lists, and its usual folders;
otherwise give it with `--game "D:\...\Transformers Fall of Cybertron"`) and writes
into `assets` (`--assets <folder>` for another; into another folder it also copies the demo's
own shaders, `assets\shaders`' hand-written ones, which aren't built from the game,
so that folder runs as it is). Each of its steps (`--list` lists them) exports
what it needs from the game's packages into `extract_work\exports`, runs its
script and logs to `extract_work\logs\<step>.log`; an export is deleted as soon as
no step still to come needs it. A step that fails stops the run: run it again and
it carries on from that step (`--from <step>` starts from another, `--only
<step>,<step>` runs just those, `--redo` starts over). The whole run took 77 minutes
on the slow two-core machine it was tested on (the longest steps: the credits movie's
encoding for the menus, 20 minutes, the sounds, 10, and the maps' doors, 8) and
needs about 5 GB of free disk space (the assets themselves are about 2.4 GB).

Built from the same game, its files come out the same as the demo's own, or the
same in what they hold, but for a few kinds of difference that don't change what
you see: UE Viewer builds differ in the bitangent's sign they give some vertices
(about one in twenty: they read it from memory Fall of Cybertron's meshes leave
unset), the textures' block compression and the animation clips' in-between frames
can differ in their last bits (they're worked out with your system's own maths
library), each sound's Ogg file is encoded again (the same sound in every band of
frequencies, a random stream number) and the menus' fonts hold the time they were
made. Beyond those, a build from a full copy of the game has a few things the
demo's own files lack (the demo's were built over time from a copy that was
missing some of the game's files): about 30 effect and weapon textures at their
full size (most of the demo's own are 128 pixels across), the DLC Kickback's
vehicle textures (the demo's own vehicle borrows two of its robot's), the EMP
Shotgun's nodes in `robot_cust.glb`, a few sound cues' settings and two door
sounds on Downfall. And where a script changed after the
demo's own file was made (the garage's and Oblivion's fog, the effects that
distort what is behind them, the vehicles' effect sizes), the build has what the
newer script makes.

To check a build against another (for instance a fresh one in an empty folder
against the demo's own), `tools\compare_assets.py` compares two assets folders
file by file and tells the files that hold something different from those that
only differ in how they were written: text with other line endings, numbers a
rounding apart or the same lines in another order; pictures and textures within a
step or two in each colour or within block compression's noise; sounds that decode
to the same audio within encoding's noise (needs ffmpeg); models with the same
nodes, meshes, materials, clips and skins, whatever their order in the file, but
for the bitangent signs; collision with the same triangles. It prints a table,
folder by folder, and writes every file that isn't byte for byte the same to
`compare_report.txt` in the first folder, with what differs (a texture's size, the
lines only in one, the nodes only in one...), the files that differ first:

```
python tools\extract_game.py --assets C:\temp\fresh_assets
python tools\compare_assets.py C:\temp\fresh_assets assets
```

The sections below say what each script makes and how, and how the exports were
first done by hand in UE Viewer.

### Making the installer for players

For players who'd rather not install Rust, Python, ffmpeg and UE Viewer,
**`make installer.bat`** (`tools\make_installer.py`) makes one file to give them:
`installer_out\Cybertron Escalation Setup <version>.exe`, the version being the
date and time it was made (for instance `2026.11.5.1430`). Make it on this PC,
which already has what sections 1 and 3 install (Rust with its C++ tools, Python
from python.org with numpy, Pillow and fontTools, and UE Viewer, wherever
`extract assets.bat` finds it: `tools\umodel`, or on the PATH; otherwise give it
with `make installer.bat --umodel "C:\...\umodel_64.exe"`), plus **Inno Setup 6**
(free, https://jrsoftware.org/isdl.php, or `winget install JRSoftware.InnoSetup`
in a terminal). It:

1. builds `foc_esc.exe` optimized, with Microsoft's C runtime built into it, so it
   runs without anything else installed (into `target\installer`: the first time
   takes about as long as the demo's own first build, later ones a minute or two;
   `--skip-build` uses the last one, and its version), with Setup's version in it:
   the main menu shows it at its bottom left;
2. gathers what Setup installs into `installer_work\stage`: `foc_esc.exe`, the
   demo's own shaders and icon, the asset build's scripts (`tools`), UE Viewer (its
   programs, DLLs, licence and readme, into the installed copy's `tools\umodel`), a
   copy of this PC's Python (without its tests or other packages, with numpy,
   Pillow and fontTools at the versions this PC has) and the licences, and
   checks that the copied Python runs the build's scripts and its window;
3. notes the build tools' id (a hash of the build's scripts and UE Viewer) in
   `tools\installer\pipeline.txt`;
4. compiles `tools\installer\foc_esc.iss` with Inno Setup.

None of the game's files are in it: each player's PC builds its own from his copy
of the game, as yours did. ffmpeg isn't in it either (it's GPL software, so it is
fetched from its publisher instead): the builder downloads it, about 110 MB, the
first time. `make installer.bat` ends by printing Setup's size and SHA-256.

After an update, make a new one and give it to the players: it installs over the
old one, keeping their progress, builds and settings. Their game files are built
again only when the build tools changed (any of the build's scripts, or UE
Viewer); otherwise the update is ready to play at once. Setup isn't signed with a
code-signing certificate, so Windows warns about it (below).

`tools\installer\builder.py` is the window that builds the game files on a
player's PC; `python tools\installer\test_builder.py` tests it, with stand-ins for
the window and for `extract_game.py`.

### Installing the demo with its installer (for players)

1. Install **Transformers: Fall of Cybertron** from Steam (the demo is built from
   your own copy of it).
2. Run **Cybertron Escalation Setup**. Windows may say "Windows protected your
   PC", because the installer isn't from a known publisher: choose **More info**,
   then **Run anyway**.
3. Choose **Next** until it installs (it needs no administrator rights and goes
   into your own user's programs folder), then **Finish** with "Build the game
   files now" ticked.
4. A window, **Cybertron Escalation**, builds the game files from your copy of the
   game: about an hour (longer on an older PC), with about 6 GB of free space
   needed. It downloads ffmpeg first, and finds the game in Steam's folders by
   itself; if it can't, it asks you for the game's folder (the one with
   `TransGame` in it). You can use your PC meanwhile, and it stops the PC from
   going to sleep until it's done. Closing it stops it; the next time it carries
   on from where it stopped (the demo's shortcut, or "Build the game files" in the
   Start menu, opens it again).
5. When it says **All done!**, choose **Play**. From then on the desktop shortcut,
   **Cybertron Escalation**, starts the demo.

If a part of the build goes wrong, the window says which, with **Try again** (it
carries on from that part) and **Show the log**: send that log to whoever gave you
the demo. To uninstall, use Windows' Settings > Apps (Installed apps); it asks
whether to keep your progress, builds and settings for next time.

## 4. Extract models from your game (by hand)

The game is built on Unreal Engine 3, so you use **UE Viewer (umodel)** to open
its packages.

1. Download UE Viewer for Windows from https://www.gildor.org/en/projects/umodel
   and unzip it.
2. Run `umodel.exe`. In the startup window:
   - **Path to game files:** `D:\Program Files\Steam\steamapps\common\Transformers Fall of Cybertron`
   - Tick **Override game detection** and pick *Unreal engine 3 → Transformers: Fall of Cybertron*.
3. Find the characters. The game was "cooked", which means there are **no
   separate character packages**: each character mesh is stored inside the
   level and cutscene packages that use it, in `TransGame\CookedPC` (names like
   `L05_Soundwave_VIG_m`). To find them:
   - In umodel's package window, click **Tools → Scan content**. This adds
     columns with how many skeletal meshes, static meshes and textures each
     package holds.
   - Click the skeletal-mesh column header to sort, so packages with characters
     rise to the top. Packages ending in `_VIG` (cutscenes) are good places to
     look because they hold the characters that appear on screen.
   - Open a package and use PgUp/PgDn in the viewer to flip through objects
     until you find the one you want.
4. Open **Tools → Options** (or the export options when exporting) and set the
   mesh export format to **glTF 2.0**.
5. Export the object (**Tools → Export current object**). You get a `.gltf` file
   plus textures in the export folder.

### Clean it up in Blender (recommended)

umodel's glTF output usually needs a little fixing (scale, facing direction,
textures not hooked up). Blender (free) is the easiest way:

1. **File → Import → glTF 2.0**, pick the exported `.gltf`.
2. If textures are missing, assign the exported diffuse/normal textures to the
   material in the Shading tab.
3. Make the model about 2–8 metres tall and standing on the ground at the origin,
   facing **+Y in Blender** (along the green axis arrow; that becomes "forward" in Bevy).
4. **File → Export → glTF 2.0**, format **glTF Binary (.glb)**.

### Converting the exports (already done for you)

umodel's glTF files can't go into the game as they are: characters come in
separate pieces, textures aren't linked, and the textures are packed in the
game's own format (`_Color_NormX` holds colour plus half of the normal map,
`_Masks_NormY` holds glow/shine masks plus the other half). The script
`tools\build_assets.py` fixes all of that and writes ready-to-load `.glb` files:

```
assets\models\robot.glb          Jazz: legs, chest, head, arms + Neutron Assault Rifle, Subsonic Repeater, Photon Burst Rifle, Scatter Blaster, Nucleon Charge Rifle, Path Blaster, Riot Cannon, Orbital Pistol and the eight weapons after them (The Titan's weapons, below), one skeleton
assets\models\robot.bin          the second half of robot.glb's data, where the copy on your PC has one (tools\split_glb.py)
assets\models\weapons\*.glb      the weapons as they lie on the ground (pickups), with cyan outlines
assets\models\vehicle.glb        Jazz's car, with simple tyres hung on its wheel bones
assets\models\enemies\*.glb      Escalation's enemies (tools\build_enemies.py: see *Escalation's enemies* below)
assets\models\robot_deaths.*     the players' death clips and ragdoll (tools\build_deaths.py)
assets\models\enemies\deaths.txt the death sets everyone dies by (tools\build_death_sets.py)
assets\models\floor.glb          floor using the sewer-ledge metal texture
assets\models\sky.glb            Iacon sunset sky dome
assets\models\fx\ion_core.glb    Subsonic Repeater flash/tracer core mesh (tools\build_fx_meshes.py)
assets\models\props\*.glb        wall, vent, pipes, piston tower, energon cube, health station base, ammo crate
```

It also turns the models to face forward and bakes an aiming pose (right arm
raised with the blaster forward), because the exported rest pose has the arms
spread out, and hangs Jazz's robot-form wheels (`RB_Jazz_STAT_Wheel`, his
`RB_Jazz_PART_Wheel` mesh part) on the game's wheel sockets: one on each forearm and
one behind each knee (the right forearm's is hidden while a gun replaces that forearm).

With all sixteen guns `robot.glb` is over 40 MB, so the copy sent to your PC is split
in two by `tools\split_glb.py`: `robot.glb` keeps its JSON and the first half of its
binary data, and `robot.bin` beside it holds the rest (glTF lets a `.glb` keep a
buffer in a file of its own; Bevy and the tools read it as one model). Rebuilding
with `build_assets.py` writes a single `robot.glb` again, which works just as well
(`robot.bin` is then unused).

**Sizes.** The map and the characters are at the game's own scale (one Unreal unit
is 1 cm). The robot meshes come out of umodel
about 6 m tall, but the game draws Jazz at 0.6 of that (the `MeshScale` of his
`RB_Jazz_MESHSET`), so `ROBOT_SCALE` in `src\main.rs` makes him about 3.6 m tall,
with the game's collision cylinder (3.1 m tall, 1.4 m radius). The car is drawn at its own
size, 3.4 m long with 30 cm wheels (`VEHICLE_SCALE`; `vehicle.glb` itself is
built 2.2 times larger so the robot's wheel mesh fits it), and the attack drones
at their mesh size, about 1.6 m across. The camera uses the game's distances and
offsets (5.5 m behind and 1.65 m to the right of Jazz, 5 m behind the car; the
Nucleon Charge Rifle's scope brings it in to 1.75 m behind, 1 m to the side and 2 m up;
a Destroyer's are his class's own, further out: *The Destroyer's characters*). As he
transforms, it moves over from the one form's place to the other's over a second and
a half, as the game's camera blends into its new strategy, rather than jumping.

To re-run it, for example after exporting more, you need Python with `numpy` and
`Pillow` (`pip install numpy pillow`):

```
cd Documents\foc_esc\tools
python build_assets.py "..\..\TFOC Project\umodel\UmodelExport\L03b_CutAndRun_BASE_m" ..\assets
```

To use a different character, edit the part list and the `TEXSETS` table (which
material uses which texture set) at the top of `build_assets.py`.

**The arm inside each gun.** The weapon meshes carry the part of Jazz's arm the
gun is built into, with its own texture set, `WEP_ArmUp_TEXSET`. The map package
holds only its small mips (128 x 128); the full 512 x 512 pixels are in
`WEP_AssaultRifleArm_p.xxx`, so with that package in the `CookedPC` folder export
the two textures into the level's export folder:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx WEP_ArmUp_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx WEP_ArmUp_TEXSET_Masks_NormY Texture2D
```

**The Photon Burst Rifle** lives in `all_shared.xxx`, not in the level packages.
Export its mesh, textures and animation set into a folder called `all_shared` next
to the level's export folder (or point `FOC_SHARED_EXPORT` at it). Its textures'
pixels are in `WEP_BurstRifle_p.xxx`, which has to be in the `CookedPC` folder:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_BurstRifleR_SKEL
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_BurstRifle_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_BurstRifle_TEXSET_Masks_NormY Texture2D
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_PulseRifle_ANIM AnimSet
```

**The Scatter Blaster** is in `all_shared.xxx` too, its textures' pixels in
`WEP_Shotgun_p.xxx` (also needed in `CookedPC`). Export its two meshes (on the right
and left forearm), its texture set and its animation set into the same `all_shared`
folder:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_ScatterblasterR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_ScatterblasterL_SKEL SkeletalMesh
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_Shotgun_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_Shotgun_TEXSET_Masks_NormY Texture2D
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_Shotgun_ANIM AnimSet
```

**The Nucleon Charge Rifle** is in `L03b_CutAndRun_BASE_m.xxx` with the rest (the
game's `RB_SniperRifleR_SKEL` and `RB_SniperRifleL_SKEL`, on the right and left
forearm, and its `Weapon_SniperRifle_ANIM`); its textures' full-size pixels are in
`WEP_SniperRifle_p.xxx` (the level package only has 128 x 128 ones), which has to be in
`CookedPC`:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx RB_SniperRifleR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx RB_SniperRifleL_SKEL SkeletalMesh
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx WEP_SniperRifle_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx WEP_SniperRifle_TEXSET_Masks_NormY Texture2D
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx Weapon_SniperRifle_ANIM AnimSet
```

**The Path Blaster** is in `L03b_CutAndRun_BASE_m.xxx` too (the game's
`RB_HeavyPistolR_SKEL` and `RB_HeavyPistolL_SKEL` and its `Weapon_HeavyPistol_ANIM`).
Its mesh's material is `WEP_IonBlaster_MATINST`, on the texture set of that name,
whose full-size pixels are in `WEP_HeavyPistol_p.xxx`, which has to be in `CookedPC`:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx RB_HeavyPistolR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx RB_HeavyPistolL_SKEL SkeletalMesh
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx WEP_IonBlaster_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx WEP_IonBlaster_TEXSET_Masks_NormY Texture2D
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx Weapon_HeavyPistol_ANIM AnimSet
```

**The Riot Cannon** is in `all_shared.xxx`, as the Scatter Blaster is (the game's
`RB_BazookaR_SKEL` and `RB_BazookaL_SKEL`, on the right and left forearm, its
`WEP_RiotCannon_TEXSET` and its `Weapon_Bazooka_ANIM`); its textures' full-size
pixels are in `WEP_Bazooka_p.xxx`, which has to be in `CookedPC`. Export them into the
same `all_shared` folder:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_BazookaR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_BazookaL_SKEL SkeletalMesh
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_RiotCannon_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_RiotCannon_TEXSET_Masks_NormY Texture2D
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_Bazooka_ANIM AnimSet
```

**The Orbital Pistol** is in `all_shared.xxx` too (the game's
`RB_SysCorruptorR_SKEL` and `RB_SysCorruptorL_SKEL`, the campaign's SysCorruptor gun,
and its `Weapon_TracerDart_ANIM`, holding the SysCorruptor's clips); its texture set,
`WEP_SysCorrupter_TEXSET` (so spelt), is exported from `WEP_TracerDart_p.xxx`, which
has to be in `CookedPC`, and its two pictures copied into the `all_shared` folder's
`Texture2D`:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_SysCorruptorR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_SysCorruptorL_SKEL SkeletalMesh
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_TracerDart_ANIM AnimSet
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport WEP_TracerDart_p.xxx
copy UmodelExport\WEP_TracerDart_p\Texture2D\*.png UmodelExport\all_shared\Texture2D\
```

`build_assets.py` adds them to `robot.glb` and also writes the weapons as they lie on
the ground (`models\weapons\*.glb`): each gun as Jazz holds it, without his arm, laid
level (Jazz's aiming pose holds his forearm pitched down some 7 degrees and rolled
some 16, so the gun is turned to point straight ahead with its top straight up:
`pickup_levelling`), with its glow areas lit in the energon cyan of the game's
`PICKUP_*_MATINST` materials and the game's cyan outline around it. The outline is a shell around the gun, pushed out
1.5 cm along its normals and turned inside out, so only its far side shows, around
the gun's edges; it glows through the bloom (`OUTLINE_WIDTH`, `OUTLINE_GLOW`).

The health stations' base (`models\props\health_base.glb`) is the game's
`ESC_PickupBase_Health_STAT`, from Downfall's own package (every Escalation map's
has it), with the pickup bases' texture set from `ENV_MP_PropsShared_p.xxx` (where
its full-size textures are), its lights glowing as the game's material has them.
Export them next to the other
exports (umodel names the folders after the packages, which is where
`build_assets.py` looks), then run `build_assets.py` as above:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport ESC_Iacon_BASE_m.xxx ESC_PickupBase_Health_STAT StaticMesh
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Masks_NormY Texture2D
```

The cube over it is the energon cube model at 0.6 times its size, as the game's
health pickup effect (`HealthPickup_FX`) shows it. Without the base model the demo
stands a plain disc in for it.

**The energon cube** (`models\props\energon.glb`, the health stations') is `PROP_NEU_EnergonCube_STAT`, drawn in its two materials by
`src\energon.rs`. Its sides are `PROP_NEU_EnergonCube_Clouds_MAT`, which
`assets\shaders\energon_cube.wgsl` follows step by step: unlit, its cyan `CoreColor`
(0, 0.82, 0.82) squared plus a swirl of colour clouds (`fractalMaps`, read where a
second, panning read of itself pushes it, so the clouds drift by the shader's own
clock), squared again and
four times as bright; where a side faces the camera head on (the tenth power of the
facing ratio, over the cube's rounded normals) it turns pink and violet, as in the
game. Its edges are `PROP_NEU_EnergonCube_EdgeGlow_MAT`, unlit white at three times
full brightness. `build_assets.py` puts `fractalMaps` in the model (its pixels are
in `PROP_NEU_Pickups_p.xxx`); export it next to the level's export first:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport PROP_NEU_Pickups_p.xxx fractalMaps Texture2D
```

Without it the sides are plain cyan. The game's cube lights nothing; the demo's
casts a cyan light around it (as bright as the pink one it had before).

**The ammo crate** (`models\props\ammo_crate.glb`, the ammo stations') is the
game's `AmmoPickupLight_STAT` (the mesh `TnAmmoCratePickup` shows over its station)
from `all_shared.xxx`, in its texture set (`PROP_NEU_AmmoPickup_NEW_texset`, whose
full-size mips are in `PROP_NEU_Pickups_p.xxx`), drawn as the weapons on the ground
are: its glow areas lit cyan and the pickups' glowing outline around it (brighter
than the weapons', as its material's `OutlineIntensity` is). Export them into the
all_shared export, then run `build_assets.py` as above:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx AmmoPickupLight_STAT StaticMesh
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx PROP_NEU_AmmoPickup_NEW_texset_Color_NormX Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx PROP_NEU_AmmoPickup_NEW_texset_Masks_NormY Texture2D
```

Without it a glowing block stands in for the crate. The stations themselves (where
they are, their price and respawn time) are the `ammo` lines `build_map.py` writes
in each map's points file, from the map's ammo pickup factories.

### Animations

The animations were exported from umodel by opening a Jazz mesh, cycling to an
animation set with **Ctrl+A**, turning retargeting off with **Ctrl+R**, and
exporting with **Ctrl+X** (umodel saves the mesh with every animation in that set).
Five sets are used, saved under `UmodelExport\Animations\<set name>\`:

| Set | Clips used in the demo |
|---|---|
| `Shared_ROBO_NAV_ANIM` | `Aim_Rest_F_C` (idle stance), `Nav_Sprint_F` |
| `Shared_SCOUT_ROBO_ANIM` | `Nav_StrafeJog_Rest_FL/FR/L/R/BL/BR` (jogging), and from its `.psa` `_F`, `_b`, `_LB`, `_RB` (straight forward and back, the sides with the hips turned back) |
| `Shared_ROBO_NAV_ANIM` | `Nav_Land`, `Nav_Land_Hard_01`, `Nav_Boost_F/B/L/R`, `Nav_Air_Boost_L/R/F/B` |
| `Shared_ROBO_NAV_ANIM` | `Nav_TakeOff_01` into `Nav_Jump_Descent` (jumping), `Nav_Jump_Descent_02` (falling off a ledge): the clips Jazz's acrobatics set, `Bumblebee_JazzAcrobatics`, uses |
| `Shared_ROBO_ANIM` | `Melee_ShieldBash`, `Melee_Boost`, `Grenade_Throw_MP`, `HitReaction_Descent` (a Leaper's blow; an enemy's melee blow plays `HitReactions_ROBO_ANIM`'s, from `robot_extras.glb`: `src\hitreact.rs`) |
| `Death_Shared_ANIM` | `Death_FrontShot_05` |
| `Shared_ROBO_NAV_ANIM` | `Aim_Shoot_F_C`, `Aim_FineAim_F_C` (shooting and fine-aim stances) |
| `Shared_ROBO_NAV_ANIM` (`.psa`) | `Nav_Rest_IdlePivot90_L/R`, `Nav_Shoot_IdlePivot90_L/R`, `Nav_Shoot_IdlePivot180_L/R`, `Nav_FineAim_IdlePivot90_L/R` (turning on the spot), and `Aim_Rest/Shoot/FineAim_L_C` and `_R_C` (the upper body turned left and right) |
| `Shared_SCOUT_ROBO_ANIM` | `Nav_StrafeJog_Shoot_*` (jogging while shooting), `Nav_StrafeWalk_FineAim_*` (walking while aiming), all ten directions of each |
| `WeaponArm_ROBO_ANIM` | `ADD_Weapon_Recoil_RapidSmall_Light_01_WEP` (arm recoil) |
| `Weapon_AssaultRifle_ANIM` | `ADD_Weapon_AssaultRifle_Fire_WEP`, `_CoolDown_WEP`, `_Idle_WEP` (the rifle's moving parts) |
| `Weapon_IonBlaster_ANIM` | `ADD_Weapon_SubsonicRepeater_Fire_WEP`, `_Idle_WEP` |
| `Weapon_AssaultRifle_ANIM`, `Weapon_IonBlaster_ANIM` | `ADD_Weapon_AssaultRifle_Reload_S_WEP`, `ADD_Weapon_SubsonicRepeater_Reload_S_WEP` (reloads) |
| `Weapon_PulseRifle_ANIM` (all_shared) | `ADD_Weapon_PulseRifle_Fire_WEP`, `_Idle_WEP`, `_Reload_S_WEP` (the Photon Burst Rifle) |
| `Weapon_AssaultRifle_ANIM`, `Weapon_IonBlaster_ANIM`, `Weapon_PulseRifle_ANIM` | `ADD_Weapon_AssaultRifle_Equip_WEP` / `_Unequip_WEP`, `ADD_Weapon_SubsonicRepeater_Equip_WEP` / `_Unequip_WEP`, `ADD_Weapon_PulseRifle_Equip_WEP` / `_Unequip_WEP` (switching weapons) |
| `WeaponArm_ROBO_ANIM` | `ADD_Weapon_Recoil_LightSmall_01_WEP` (the Burst Rifle's recoil, once per round at 1.5x) |
| `Weapon_Shotgun_ANIM` (all_shared) | `ADD_Weapon_Scatterblaster_Fire_WEP` (at 0.9x, with `ADD_Weapon_Recoil_LightSmall_01_WEP` at 1.667x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the Scatter Blaster) |
| `Weapon_SniperRifle_ANIM` (L03b_CutAndRun_BASE_m) | `ADD_Weapon_SniperRifle_Fire_WEP` (with `ADD_Weapon_Recoil_HeavyLight_Small_01_WEP` from `WeaponArm_ROBO_ANIM` at 2x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_UnEquip_WEP`, `_MirrorReverse_WEP` (the Nucleon Charge Rifle) |
| `Weapon_HeavyPistol_ANIM` (L03b_CutAndRun_BASE_m) | `ADD_Weapon_PathBlaster_Fire_WEP` (with `ADD_Weapon_Recoil_LightSmall_01_WEP` from `WeaponArm_ROBO_ANIM` at 2.25x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the Path Blaster) |
| `Weapon_Bazooka_ANIM` (all_shared) | `ADD_Weapon_RiotCannon_Fire` (with `ADD_Weapon_Recoil_HeavyLight_Small_01_WEP` from `WeaponArm_ROBO_ANIM` at 1.8x, once per shot), `_Idle`, `_Reload_S`, `_Equip` / `_Unequip`, `_MirrorReverse` (the Riot Cannon) |
| `Weapon_TracerDart_ANIM` (all_shared) | `ADD_Weapon_SysCorruptor_Fire_WEP` (with `ADD_Weapon_Recoil_LightSmall_01_WEP` from `WeaponArm_ROBO_ANIM` at 2x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the Orbital Pistol) |
| `Shared_SCOUT_ROBO_ANIM` (`.psa`) | `Nav_Idle_HeavyWep_Shoot`, `Nav_StrafeJog_HeavyWep_Shoot_*` and `Nav_StrafeWalk_HeavyWep_Shoot_*` (all ten directions of each), `Nav_Shoot_HeavyWep_IdlePivot90_L/R` and `_IdlePivot180_L/R` (the heavy-gun stance) |
| `Weapon_HeavyMG_ANIM` (all_shared) | `ADD_Weapon_HeavyMG_Fire_WEP` (with `ADD_Weapon_Recoil_RapidSmall_Heavy_01_WEP` from `WeaponArm_ROBO_ANIM`, looping while it fires), `_WarmUp_WEP`, `_CoolDown_WEP`, `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the X18 Scrapmaker) |
| `WEP_EMPShotgun_ANIM2` (all_shared) | `ADD_Weapon_EMPShotgun_Fire` with `ADD_Weapon_EMPShotgun_Recoil_S` (for each shot of a burst), `_Idle02`, `_Reload_S`, `_Equip` / `_Unequip`, `_MirrorReverse` (the Energon Harvester) |
| `Weapon_RepairBeam_ANIM` (all_shared) | `ADD_Weapon_RepairBeam_Fire_WEP` (with `ADD_Weapon_Recoil_RapidSmall_Light_01_WEP` at 0.8x, looping while the beam is on), `_CoolDown_WEP`, `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the E.D.K. TechVolt) |
| `Weapon_RustCannon_ANIM` (all_shared) | `ADD_Weapon_RustCannon_Fire` (with `ADD_Weapon_Recoil_LightSmall_01_WEP` at 2x, once per shot), `_Idle`, `_Reload_S`, `_Equip` / `_Unequip`, `_MirrorReverse` (the Corrosive Slime Cannon) |
| `Weapon_Sawblade_ANIM` (all_shared) | `ADD_Weapon_Sawblade_Fire_WEP` and `_ChargedFire_WEP` (each with `ADD_Weapon_Recoil_HeavyLight_Small_01_WEP` at 1.667x, once per shot), `_Charging_WEP` (looping while it charges), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the Gear Shredder) |
| `Weapon_RocketLauncher_ANIM` (all_shared) | `ADD_Weapon_RocketLauncher_Fire_WEP` (with `ADD_Weapon_Recoil_HeavySmall_01_WEP` at 1.5x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the Thermo Rocket Cannon) |
| `Weapon_GrenadeLauncher_ANIM` (all_shared) | `ADD_Weapon_PulsarCannon_Fire_WEP` (with `ADD_Weapon_Recoil_HeavyLight_Small_01_WEP` at 1.5x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the A-4 Pulsar Cannon) |
| `WEP_GrenadeArm_ANIM` (all_shared) | `ADD_Weapon_GrenadeLauncher_Fire_WEP` (with `ADD_Weapon_Recoil_HeavyLight_Small_01_WEP` at 2x, once per shot), `_Idle_WEP`, `_Reload_S_WEP`, `_Equip_WEP` / `_Unequip_WEP`, `_MirrorReverse_WEP` (the Chaos-Rift Combustor) |
| `Jazz_Arm_ROBO_ANIM` (`.psa`) | `ADD_Weapon_MirrorReverse_ARM` (Jazz's forearms changing over as he switches hands) |
| `Jazz_ROBO_ANIM_NEW` | `Transform_ToVehicle_ROBO`, `Transform_ToRobot_ROBO` |

umodel's glTF export of `Shared_ROBO_NAV_ANIM` silently leaves out 22 of its 116
clips (including the forward/back air dashes and the straight forward/back jogs).
Exporting the AnimSet on its own as an ActorX `.psa` with a fixed umodel keeps them
all; it is saved as `Animations\Shared_ROBO_NAV_ANIM\Shared_ROBO_NAV_ANIM.psa`, and
`build_assets.py` reads clips from `.psa` files as well (`tools\psa_io.py`).
The weapon sets and `Shared_SCOUT_ROBO_ANIM` are read from `.psa` files saved the
same way (`Animations\<set name>\<set name>.psa`); the scout set's glTF export lacks
the forward-diagonal fine-aim walks.

The `ADD_*` clips are *additive*: they hold changes to whatever pose is playing,
not poses of their own, which is how the game layers recoil and the gun's moving
parts over any stance or move. `build_assets.py` combines them into ten additive
clips (`rifle_fire`, `rifle_cooldown`, `rifle_idle`, `ion_fire`, `ion_idle`,
`rifle_reload`, `ion_reload`, `burst_fire`, `burst_idle`, `burst_reload`; `burst_fire`
holds a whole three-round burst, the recoil starting again with each round), and
`main.rs` plays them under an additive blend node on top of the other clips. The
game limits the gun-part animations to the gun's own bones, and so does the script.
A gun's own clips take turns, as in the game, which plays them all in the robot anim
tree's `WeaponCustom` slot (`TnAnimNodeWeaponSlot`, the `CAMT_Weapon` mask): its idle
loop (`TnWeaponEffectIdleAnimation`) plays only while none of its others does (the
firing loop, a shot's or a burst's clip, the reload or a spin-down, the gun unfolding
or switching hands), stopping for them and starting again from its first frame after
them (`TryToPlayIdleAnimation`). Added together, two of them move a part twice: the
Orbital Pistol's orbiting parts, which its idle swings round the gun, flew off it as
it fired and reloaded.

`build_assets.py` copies those clips into `robot.glb` (matching bones by name and
removing umodel's root-motion track, since the game moves Jazz itself). Their order
is fixed: the `ANIM_*` constants at the top of `src\main.rs` refer to them by index.
Each model file is loaded once and its clips put into the animation graphs as it
comes in (`GltfClips` in `main.rs`): asked for clip by clip (`robot.glb#Animation7`)
before the file is in, Bevy loads the whole file again for every clip, and with
Jazz's hundred clips, his mirrored set and the car's, that was some 230 loads of the
models at startup, gigabytes of memory, and the loading threads busy for a long
while.
The extra strafe clips (each stance's straight forward/back and hips-back sides) come
after the additive ones (`ANIM_STRAFE_EXTRA`), so the earlier indices stay as they were,
then the pivot clips (`ANIM_PIVOT`, `ANIM_PIVOT_180`) and the upper-body turns
(`ANIM_TWIST`). The pivot clips turn Jazz by turning their reference bone, which is
left out like every clip's; `main.rs` turns him by the same amount, evenly over the
clip, as the clip does. The upper-body turns are additive clips the script makes
from the game's aim offset poses (each stance's pose turned fully left or right
less its straight-ahead one); the demo no longer plays them (they keep their place
so the clips after them keep their numbers): `src\robot_aim.rs` puts the robot anim
tree's own aim offset node on his bones instead. That node
(`TR_Shared_ANIMTREE_p.Robot_ANIMTREE`'s `TnAnimNodeAimOffset`, read from
`MP_Array_BASE_m.xxx` by `build_robot_extras.py` into `robot_extras.txt`) holds, for
each of its profiles, each bone's offset for the nine ways it aims (left, ahead or
right; up, level or down), which the game baked from the profile's aim poses
(`Aim_Shoot_*` for the Default profile, `Aim_FineAim_*` for fine aim): a turn in
the mesh's space and a move. Each frame the four offsets round the aim are blended
and put on the bones from the top of the skeleton down (UE3's
`AnimNodeAimOffset.GetBoneAtoms`, as the enemies' nodes are: `src\ai\aimoffset.rs`);
from `Aim_*_F_C` they give back the game's other aim poses exactly. The aim is the
camera's from where Jazz faces, a quarter turn each way, eased in at the node's
`InterpSpeed` (12); the Default and fine-aim profiles stretch the pawn's aim within
their `PawnAimOffsetRange` (from the hip 54° down to 62° up) over their whole aim
(`RemapPawnAimRange`). Left-handed, the aim is switched side to side and each offset
goes to its bone's partner on the other side, reflected, as the mirror node over it
mirrors the whole pose.
Last come the weapon switches (`ANIM_EQUIP`), additive: each gun's `Equip_WEP` clip
unfolds it out of Jazz's forearm on the gun bones (`R_CloudA_*`, the game's
`CAMT_Weapon` mask) and its `Unequip_WEP` folds it back in on his second set of gun
bones (`R_CloudB_*`; the game plays it `PlayOnCloudB`). So the old gun can fold away
while the new one unfolds, `build_assets.py` gives each gun a second mesh skinned to
the CloudB bones (`<mesh>_CloudB` in `robot.glb`), shown only while it folds away.
After those come the hand switches (`ANIM_HAND_SWITCH`, one `MirrorReverse_WEP` per
weapon), then Jazz's forearms changing over (`ANIM_ARM_SWITCH`): the game's forearm
parts append `Jazz_Arm_ROBO_ANIM`, whose `ADD_Weapon_MirrorReverse_ARM` folds one
forearm away and unfolds the other on the CloudB arm bones, its `ScaleBone` notifies
shrinking each piece to nothing or growing it back (`main.rs` does the same in
`fold_forearms`, `switch_bone_scale`). So `robot.glb` also has a copy of each forearm
and its wheel on the CloudB bones (`RB_Jazz_CHARSKEL_ArmLoL_CloudB`,
`L_CloudB_ElbowRobo16_XT_Wheel` and the right ones), shown only while he switches.
The set comes from the level package as an ActorX `.psa`:

```
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport L03b_CutAndRun_BASE_m.xxx Jazz_Arm_ROBO_ANIM AnimSet
```

kept as `Animations\Jazz_Arm_ROBO_ANIM\Jazz_Arm_ROBO_ANIM.psa` (without it the
forearms only shrink away and grow back in place, without folding).
For switching hands, `build_assets.py` also writes `robot_mirror.glb`: every one of
those clips again, mirrored left for right by `tools\mirror_anims.py`, in the
same order, with only Jazz's skeleton in it (`main.rs` plays both sets and blends
between them). It mirrors each bone's pose across the middle of the skeleton onto
its partner on the other side, corrected so the symmetric bind pose mirrors onto
itself. Each mirrored clip lasts exactly as long as its original (bones a clip
doesn't move are held for the clip's own length), so the looping moves cycle at
the same rate on either arm and stay in step with the unmirrored set.
One of the game's loops doesn't close on every bone: the backward-left fine-aim walk
(`Nav_StrafeWalk_FineAim_BL`) starts with the left arm lowered and raises it over
its first four frames. Looped as it is, the arm would drop and swing back up every
cycle, so `build_assets.py` bridges those frames from the clip's end for the bones
that jump (`LOOP_SEAM_FRAMES`, `close_seam`). `robot.glb` also carries the left-handed gun meshes (`RB_AssaultRifleL_SKEL`,
`RB_IonBlasterL_SKEL`, `RB_BurstRifleL_SKEL`, export them from
`L03b_CutAndRun_BASE_m.xxx` like the right-handed ones; without them Jazz can't
switch hands), each with its CloudB copy, and his right forearm (`RB_Jazz_CHARSKEL_ArmLoR`).
To swap a clip, change its name in the `anims` list in `build_assets.py` and re-run it.
The other sets (melee, grapple, weapon reloads, landing, boost) are there to use too.

The car's animations are `Jazz_VEH_ANIM`, which umodel saves as an ActorX `.psa`
(`umodel -export ... L03b_CutAndRun_BASE_m.xxx Jazz_VEH_ANIM AnimSet`, kept as
`Animations\Jazz_VEH_ANIM\Jazz_VEH_ANIM.psa`). `build_assets.py` puts them into
`vehicle.glb`: the reference pose the car boosts on its wheels in (Jazz's set has no
`Nav_Boost_VEH`), the car's halves of the
transformations (`Transform_ToVehicle_VEH`, `Transform_ToRobot_VEH`), the hover poses
(`Nav_Hover_Pose_VEH` and its F/B/L/R versions), the switches between driving and
hovering (`Nav_BoostToHover_VEH`, `Nav_HoverToBoost_VEH`), and the additive hover bob
and gun equip/unequip. The car's guns are their own mesh in the game,
`WEP_AssaultRifleVH_SKEL` (export it from `L03b_CutAndRun_BASE_m.xxx` like the other
meshes); `build_assets.py` mounts one on each of the car's weapon sockets, and their
clips (`WEP_AssaultRifleVH_ANIM`, saved as
`Animations\WEP_AssaultRifleVH_ANIM\WEP_AssaultRifleVH_ANIM.psa`) unfold them, spin
them while firing and fold them away. Through a transformation the guns are hidden, as
in the game: going to the car they stay away for its first second
(`TransformToAltShootingDelay`, 1.0, before `SwitchToFormWeapon` gives the car its
weapon), and going back to the robot they go the moment it starts
(`UnEquipWeaponInstantly`), in matches and in the Customize preview. `src\car.rs` plays them the way the game's car animation tree
(`VEH_Proxy_p.VEH_Car_ANIMTREE`) does, and turns, steers and springs the wheels as its
SkelControlWheels do, with Escalation's numbers: on the wheels 15 degree
steering, 30 cm wheels, 35 cm of suspension travel and a 15 m/s jump that lifts the
nose (`Car_Physics`); hovering about as high as on its wheels (its four hover springs
and its four wheels' bear the same 1500 kg and come to rest within 1.5 cm of each
other, as the game's `TnSpring` steps them: the trucks 15.5 cm higher, Swindle's and
Hound's jeeps half a metre), 15 m/s strafing, a 15.5 m/s jump
with a 5 rad/s nose-up kick, and half a second of drift after the wheels
(`HoverCar_Physics` and the `TnHoverCarSimulationBlueprint` defaults). How the
car drives on its wheels is `src\drive.rs`'s (`TnCarSimulation`'s push, drag and
banking, its `TnWheelAssembly` tyres) and how the camera follows it
`src\vehicle_cam.rs`'s (the class's camera set's driving and hovering strategies,
which `tools\build_class.py --drive` writes into the class's data with the cars' and
trucks' physics). Left out: the camera behaviours whose workings are only in the
game's native code beyond their names and numbers (the driving camera's lead on
activation, `LeadingYawOffsetOnActivation`, and the trucks' `TnOrbitFollowCameraBehavior`),
and the Nitro ability's super boost. The hover poses blend by the car's
velocity as the tree's `TnAccelerationAnimBlend` does (each axis squared over the
speed, up to 20 m/s).

Which of the two the car is in, and how each moves, comes from the game's scripts in
`TransGame.xxx`: `TnCarForm` hovers (its `Hovering` state) until boost is held, then
drives (`Driving`, which holds the throttle full and plays `BoostJumpAnim`/`BoostLandAnim`,
both unset for Jazz, as are his hover jump and landing animations). They are
UnrealScript bytecode; `tools\uscript.py` turns a function back into readable form,
for example:

```
python tools\uscript.py "C:\...\TransGame\CookedPC\TransGame.xxx" TnCarForm.Hovering.UpdateJumping
```

Without the `.psa` files, `vehicle.glb` is built without animations and the car just
drives.

### The Escalation maps

The demo's four maps are the game's Escalation maps Downfall, Oblivion, Ancients
and Ignition. Each is split in two packages in `TransGame\CookedPC`: `<map>_ART_m.xxx`
(the level: its geometry and lighting) and `<map>_BASE_m.xxx` (gameplay: spawn
points, AI paths, the consoles, doors and pickups). umodel exports the meshes and
textures but not how the level places them, and it can't read the BSP floors and
walls or a terrain at all, so `tools\build_map.py` reads the packages directly, with
the scripts in `tools\ue3`:

| Script | What it reads |
|---|---|
| `layout.py` | every placed static mesh (position, rotation, scale, material overrides), which texture set each material uses, and each mesh section's material |
| `bsp.py` | the level's BSP: floors, walls, ceilings and their materials |
| `points.py` | the player start, Escalation spawn points, AI path nodes and health stations |
| `lighting.py` | the baked light maps (and their pages in the `_LM` packages), the lights, the sun, the fog layers, post-process settings and colour grade |
| `kismet.py` | the level's Kismet: what its Matinees move (and the animation sets their groups play from) and which transforming doors it opens when the consoles are bought, and what it hides as gameplay starts |
| `terrain.py` | a level's terrain (Ancients' floor): its grid of heights, its layers and the weight maps that paint them, and its parts' light maps |
| `anim.py` | animation clips in FoC's own compressed layout, which umodel skips for some clips |
| `texture.py` | textures umodel doesn't export (the flip books of the animated screens) |
| `matparams.py` | the parameters each material instance sets (scalars, colours and static switches) and so how its texture set glows: its `EmissiveIntensity` and `Emissive_Color` |
| `fxmat.py` | the effects' materials: a material instance's parameters and its base material's expression graph, compiled to WGSL (see "The maps' effects") |
| `particles.py` | particle systems: each emitter's settings and High Moon's compiled form of its modules |

For a map it writes, with the parts it shares with `build_garage.py`
(`tools\build_level.py`: texture-set materials, the BSP, light maps and lights):

- every static mesh the two packages place, with the level's transforms, material
  overrides and light maps (both packages' `_LM` files), the level's BSP and its
  terrain, if it has one;
- every texture at the game's full size. The game keeps its textures
  DXT-compressed with mip maps, and so does the demo here: `build_map.py` writes
  them as KTX2 files (`tools\ktx2_io.py`): BC1 for colour, BC3 for colour with
  alpha, BC5 for normal maps (the texture sets' two normal channels), each with
  all its mip maps, which Bevy loads as they are (its `ktx2` feature, on by
  default). They are filtered as the game's PC version filters them (4x
  anisotropic, its `MaxAnisotropy`);
- the glow of the texture-set materials as each one sets it: the base material
  most of the level's materials are instances of (`ENV_BRDF_A_MAT`) lights the
  texture set's glow mask (its Masks texture's red channel) at the instance's
  `EmissiveIntensity`, in its `Emissive_Color` where its static switch
  `Emissive_UseColorParam` says so and in the colour map's own colours otherwise
  (`tools\ue3\matparams.py`); an instance that sets no `EmissiveIntensity` (the
  base material's is 0), or whose `Emissive_Color` is black, doesn't glow. An
  instance's parents are followed into the packages the game loads with every map
  (`all_shared.xxx` and `mp_shared.xxx`) when the map's own don't hold them:
  Downfall's `IAC_BRDF_Base_MATINST`, the parent of its upper floors' plating and
  others, is `all_shared.xxx`'s (`map_params`). (Materials of other base materials
  glow where their glow mask is bright, as before);
- the glowing, see-through, additive and animated surfaces and the backdrops from
  their own materials (`SPECIAL` in the script), which say which they are in glTF
  material extras (`{"foc": "add nofog"}`) that `src\lighting.rs` applies;
- collision from the meshes, the BSP and the terrain, plus the level's blocking
  volumes as walls that stop movement only (`src\collision.rs`'s compact format:
  rays, and so shots, sight lines and the camera, pass them), the player-only ones
  (`HmPlayerOnlyBlockingVolume`: the walls round the enemies' spawn areas) kept
  apart: they stop the players but let the enemies through, as the game's do. Each
  volume stands where the engine puts it (`AActor::LocalToWorld`): its `PrePivot`,
  the pivot an editor moved, taken off its brush before it is turned and placed
  (17 volumes have one: seven blocking volumes and five player-only ones on
  Ancients, four player-only ones on Oblivion and one on Downfall; Ancients' Loft
  blocker without it stood 56 m from its place, an invisible wall in front of the
  Loft's door, and the others 7 to 94 m off). A mesh the level
  blows up (Ancients' cliffs are rocks at 10 to 15 times their size) keeps its
  shape: its collision is simplified in the level's metres, not its own. Each
  triangle's corners go round the way the game draws it, its normal facing out, a
  mesh placed mirrored (a negative scale, which turns it inside out) turned round:
  the movement tells a rock's underside from its top by it;
- the player start, enemy spawn points, AI path nodes and health and ammo
  stations, the light maps, lights, haze, bloom and colour grade;
- the bases the purchase points stand on, their lights in the colour of what the point
  sells (each kind of point has its own base: `ESC_PickupBase_*`, instances of the
  pickups' `Pickup_Unlit_MAT` whose lights glow in their `Emissive_Color` at their
  `EmissiveIntensity`, `ue3\matparams.py`'s pickup_glow; drawn unlit as the game's
  are: `src\pickupbase.rs`): cyan for the weapons and
  the ammo, yellow for the heavy weapons and the heavy ammo, red for the assault
  equipment (the Thermo Mine Payload, the Dimensional Decimator) and blue for the
  utility equipment (the Diffraction Barrier, the Energon Recharger, the
  Intercooler); and the purchase points themselves in the
  map's points file (`weapon`, `equipment`, `health`, `ammo` and `heavyammo` lines:
  where each stands, which way it faces, its price and its respawn time; see
  Escalation's shop below).

- the effects (`tools\levelfx.py`: see "The maps' effects"): the effect meshes with
  their game materials, the particle emitters the level starts with running, and
  its lens flares.

Delete the maps' `.glb` files
(`assets\models\downfall.glb`, `oblivion.glb`, `ancients.glb` and `ignition.glb`) to
go back to the prop arena.

`build_map.py` takes a few minutes, most of them compressing the textures. umodel
finds the textures' full-size mips in the packages they come from, which is why it
reads the whole `CookedPC` folder (`<CookedPC>` below: the game's
`TransGame\CookedPC`); `build_map.py` reads the map's packages and their `_LM`
light-map files from it too, and the packages its animated screens' flip books
come from. `python render_level.py ..\assets\models\downfall.glb view.png 960 540
81 5 -4 81 3 30` draws a quick preview from a camera position and target, without
starting the game (it reads the KTX2 textures).

`python build_map.py --volumes <map> "<CookedPC>" ..\assets` rewrites only a built
map's blocking volumes in its `levels\<map>_col.bin` (its other collision kept as it
is, the doors already taken out): that is how the four maps' player-only walls were
put apart from the others, and how Downfall's roof blocker was added.

`python build_map.py --hulls <map> "<CookedPC>" ..\assets` rewrites only the
collision of the meshes MAPS' `hulls` lists for the map in its
`levels\<map>_col.bin`: each placed in the map's model loses the triangles of its
own shape (found again in the file, as `col_winding.py` finds a placement's) and
gets its collision hulls' (the StaticMesh's BodySetup), as a full build makes them;
running it again changes nothing. That is how Downfall's steel trusses got their
boxes (below).

`python col_winding.py ..\assets downfall oblivion ancients ignition` turns round
the collision triangles of the meshes a map places mirrored, in a
`levels\<map>_col.bin` built before `build_map.py` did it itself (it makes each
mirrored placement's collision again from the map's model, finds it in the file and
turns it round; running it again changes nothing). Run it once on maps built
before; the demo's own were done with it.

### The Downfall map

Downfall (`ESC_Iacon`, the game's map 1603: a street of Iacon at sunset):

- **every area open**, as the level stands once each console has been bought: the
  level's Kismet (`tools\ue3\kismet.py`) plays the Matinees the consoles' purchases
  and the start of the match trigger, to their ends: the ordnance doors slide 15 to
  20 m into the walls, the start area's bars drop into the floor and ceiling, a
  block of cover lowers by 2 m and the retractable cover folds away
  (`CoverABIG_Activate`, played backwards). What the level hides as gameplay starts
  (its rockets) is left out;
- **its skeletal props**, posed as the level leaves them: the Escalation consoles
  (door and trap: the weapon-upgrade consoles are left out, as the upgrades are the
  builds', `tools\build_map.py`'s `SKIPPED_ACTORS`; `tools\remove_console.py` takes
  them out of maps built before) with their glowing keyboards and their screens,
  whose flip books (`AP_EscScreenFlipbook_*`, read from the packages by
  `tools\ue3\texture.py`) `src\lighting.rs` animates frame by frame at the game's
  rates; the loot pods; the retractable covers; and the three big Optimus doors at
  the streets' ends, standing open on their electric barriers. The doors'
  animation (`AP_OptimusDoor_ANIM`) is read straight from the package by
  `tools\ue3\anim.py`: umodel skips FoC clips stored as these are;
- its destructible beams, intact;
- **the sunset sky**: its dome with the sun's glow, seen through the level's haze as
  the game shows it. The game's three layers of height fog (lavender, peach and
  rust) cover the sky too, about three quarters of it straight up and all of it at
  the horizon, so `build_map.py` bakes them into the dome's vertices; over the map
  itself they are one exponential haze (their densities added, their colours
  mixed). The dome is drawn a fraction further out than the haze and glow laid
  over it (`lighting.rs`'s `SKY_BASE_PUSH`), so they never lose the depth test to
  it in patches; and the sun's light shafts stream past the towers against it (*The
  sun's light shafts*);
- the map room's holograms and animated screens, the computers' and lights' glow,
  the backdrop buildings and the electric barriers, from their own materials;
- **the spawn gates' electricity** (`ElectroPlane_MAT` on the cards across them),
  drawn by the game's own material graph as the maps' effects are (see "The maps'
  effects"): lightning panning across the cards from the three layers of `Energy01`,
  bent by a drifting cloud texture (`DiffClouds_CLR`), with the bright arcs sweeping
  through, each card out of step with the others (the game's per-mesh random seed);
- **the start room's roof closed to the players**: the hall the players start in is
  open to the sky through a light well at its west end, and a jet could fly up it
  onto the hall's roof (30.7 m up), where enemies spawn (four spawn points, 32.5 m
  up), and stay there: the game stops nothing below the map's players-only lid,
  38.4 m up. `build_map.py` (MAPS' `blockers`) adds a players-only box over the
  light well and the roof and its grates, from 30 m up into the lid (the hall's
  ceiling is at 29.4 m): a jet climbing the light well stops just under the roof,
  as under the hall's own ceiling, and nobody can stand on the roof, while the
  enemies spawning there walk through it, as through the game's spawn walls;
- **the steel trusses by their collision hulls**: the game collides with a mesh by
  its hulls (its StaticMesh's `UseSimpleBoxCollision` and `UseSimpleLineCollision`,
  on unless the mesh turns them off), the demo with the mesh's own shape, which is
  near enough for most. Not for `beam1_STAT`, the open steel truss, whose hull is a
  box round it: the enemies that spawn on the start hall's roof walk the tops of
  the four off its edges, as the game's navigation has it, to jumps off their
  sides, and the lattice let them drop in or land on its side bracing.
  `build_map.py` (MAPS' `hulls`) gives the standing trusses their boxes; the ones
  the purchases bring down keep their shape (their collision is their door
  groups').

```
assets\models\downfall.glb          the map: BSP + 2,223 placed meshes (props, doors and effect meshes included)
assets\models\downfall\*.ktx2       its 297 textures (191 MB), block-compressed with mip maps
assets\levels\downfall_col.bin      collision: 817,778 triangles, + 216 for the blocking volumes and 580 for the player-only ones and the roof blocker
assets\levels\downfall_points.txt   start point, level camera, KillZ, 112 enemy spawn points, 1,212 path nodes, 5 health stations, 6 ammo stations
assets\levels\downfall_lightmaps\   the game's baked lighting: 21 light-map pages and the vertex-light page
assets\levels\downfall_lights.txt   the sun, its light shafts, 81 point lights, haze, bloom, depth of field (read by src\lighting.rs)
assets\levels\downfall_grade.bin    the map's colour grade (VortexFlyingPrevis_CLUT)
assets\levels\downfall_fx.txt       its effects: 11 materials, 2 particle systems placed 11 times, a lens flare placed 55 times
assets\menu\img\lobby_map_downfall.png   its picture in the lobby (made by build_menu.py)
```

To rebuild it (Python with `numpy` and `Pillow`), export the two packages whole,
the covers' animation on its own (umodel's whole-package export leaves animation
sets out) and the two textures the pickup bases take from a shared package, all
into one folder, `<export>`:

```
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Iacon_ART_m.xxx
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Iacon_BASE_m.xxx
umodel -game=trans -path="<CookedPC>" -export -out=<export> ESC_Iacon_BASE_m.xxx AP_CoverABIG_ANIM AnimSet
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Color_NormX Texture2D
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Masks_NormY Texture2D
cd Documents\foc_esc\tools
python build_map.py downfall <export> "<CookedPC>" ..\assets
```

The consoles', loot pods' and Optimus doors' animations need no export: they are
in FoC's own layout, which `anim.py` reads from the packages.

### The Oblivion map

Oblivion (`ESC_Ark`, the game's map 1600, aboard the Ark; a Decepticon map in the
game, played here by Jazz as Downfall is):

- its transforming doors (`TnTransformingWorldActor`), posed as the level starts
  them, from their `AP_IntDoor_ANIM` animation;
- **its windows and the view out of them, drawn with the game's own materials**
  (their material graphs, compiled for the demo as the maps' effects are, and
  moving as the game's do: see "The maps' effects"). The windows' glass
  (`ENV__ARK__Glass3_MATINST` and `Glass2`, instances of
  `ENV_Refractive_Glass_Trans_Mat`): blue, patterned by `DecWalls_Mask`, glinting,
  with a bright rim where it is seen edge-on and a thin orange line sweeping across
  it over a scrolling texture. The energon holding tower's window
  (`TowerWindow_MATINST`, `ENV_Refractive_Glass_Opaque_Mat`): the room behind seen
  through it, bent by its normal map and tinted, reflecting the cube map the map's
  characters reflect. And through the windows, outside the haze: the space dome
  (`ENV_SpaceDome_MAT`: the game's 512 x 512 star field three times over, at one, two
  and three times its size, the first turning and the others drifting, let through by
  a cloud mask drifting across them, x 400, and the nebula drifting on its own UVs),
  the space bridge and the vortex (`ENV_RST_VortexClouds2_MAT` turning,
  its black eye `ENV_RST_VortexBlackSwril2_MAT` turning three layers of swirl as it
  zooms into them, and `Vortex_MAT_INST`'s glow), and Cybertron (`ctron_CLR_Mat`),
  lit as the game lights it: its colour map x 2 under its own light map's light, each
  of the light map's three directions on its normal map as the game's shader weighs
  them (baked into its colour: that light never changes), its red-gold city lights
  (`ctron_GLOW2` x (50, 1, 0)) and its outline cut out by its mask;
- its red energy fields (`ESC_Shield_tiled2`, waves running through their mesh) and
  energon signs, the two animated ones too (`fbook_sinewave2_CLR_Mat` and
  `fbook_sinewave6_CLR_Mat`: flip books of a sine wave, four frames, the second under
  two scan-line textures panning across it), from their game materials;
- its blocking volumes, but for the two that the level's Kismet switches off as a
  team opens a gate, which are left out with the gates.

Left out: the barriers a team pays to open in Escalation (the dropship doors and
red force fields of the BASE package: all areas are open).

```
assets\models\oblivion.glb          the map: BSP + 4,197 placed meshes (doors, windows and the space view included)
assets\models\oblivion\*.ktx2       its 333 textures (208 MB), block-compressed with mip maps
assets\levels\oblivion_col.bin      collision: 1,003,065 triangles, + 2,060 for the blocking volumes and 892 for the player-only ones
assets\levels\oblivion_points.txt   start point, level camera, 77 enemy spawn points, 1,038 pickup points, 5 health stations, 6 ammo stations
assets\levels\oblivion_lightmaps\   the game's baked lighting: 18 light-map pages and the vertex-light page
assets\levels\oblivion_lights.txt   135 point lights and a spotlight, haze, bloom, depth of field (read by src\lighting.rs)
assets\levels\oblivion_grade.bin    the map's colour grade (ENV_ARK_BridgeWarmUpdated)
assets\levels\oblivion_fx.txt       its effects: 16 materials (11 for its windows, space view, energy fields and signs), 2 particle systems placed 26 times, 6 lens flares placed 96 times
assets\menu\img\lobby_map_oblivion.png   its picture in the lobby (made by build_menu.py)
```

To rebuild it, export the two packages whole, the doors' animation on its own and
the three textures the map takes from shared packages, all into one folder,
`<export>`:

```
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Ark_ART_m.xxx
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Ark_BASE_m.xxx
umodel -game=trans -path="<CookedPC>" -export -out=<export> ESC_Ark_ART_m.xxx AP_IntDoor_ANIM AnimSet
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Color_NormX Texture2D
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Masks_NormY Texture2D
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_AllShader_p.xxx ChainLinkFence_02_MASK Texture2D
cd Documents\foc_esc\tools
python build_map.py oblivion <export> "<CookedPC>" ..\assets
```

### Oblivion's exploding barrel

The Autobot energon barrel by Oblivion's start (`ESC_Ark_ART_m`'s
`TnStaticDestructibleActor_4264`; its blueprint, `ENV_IAC_ExplodingBarrel_p`'s
`DES_AutobotExplodingBarrel_DSYS`), as the game has it (`src\barrels.rs`):

- **Standing** (`AutobotExplodingBarrel_State1`, 25 health): `AutobotExplodingBarrel_STAT`
  at its components' 0.7, its lights glowing red (`Emissive_Color` 0.41 0 0 at
  `EmissiveIntensity` 5.48; its `Emissive_Pulse_Rate` does nothing, its shaders
  having been compiled without the pulse) and the red outline round it
  (`OutlineColor` 1 0 0), solid to movement and shots. It hums as the match starts
  (`EMIT_AMB_MACH_OSC_07`, the state's sound). It is a target
  (`TnTargetableDestructionEffectFactory`): the crosshair turns red over it. The
  enemies' navigation already goes round it (the game's was built with it there).
- **Damage**: the players' rounds that strike it (every gun's and the vehicles'),
  their rounds' blasts, a charging punch or a sword swing that reaches it, and the
  enemies' rounds, rockets and grenades take their damage off its health, with the
  blueprint's modifiers: an enemy's at half (`AIDamageModifier` 0.5), a blast's at a
  quarter unless its middle is within a metre of it (`RadiusDamageModifier` 0.25,
  `MinRadiusDamageDistanceForModifier` 100), Corrosion's five times (its
  `DamageModifiers`; no gun in the demo deals it).
- **Blowing up** (`AutobotExplodingBarrel_State2`, which every kind of damage brings it
  to): the barrel and its outline give way to the wreck (`LPdamage2_STAT`, unlit), and
  its collision to the wreck's; the explosion (`Exp_GenericLvl_MED_FX` 1.76 m up) and
  its sound (`DEST_ENERGON_TANK_01_EXPLO`); its debris, flown as the deaths' gibs are
  (`HmDebrisDestructionEffectFactory`, each at its place, speed, spin, cone and
  lifespan: its own pieces `Debris1_STAT` to `Debris3_STAT` for 3 s, the big
  `Debris4_STAT` thrown upwards trailing `Wounded_Soldier_FX`'s smoke and sparks, and
  six `CoverBLockCLarge_debris_02_STAT` chunks trailing `Fiery_Smokey_GibsTrail_FX`'s
  fire); the fire on the wreck (`Fire_ExpBarrel_FX`) for the state's 10 s, after which
  the wreck stands charred for the rest of the match; and its blast
  (`HmRadialDamageDestructionEffectFactory`: 150 of `TnDamageTypeBruticusFire` to
  everything within 15 m in sight of a point 1.93 m up, all of it at any distance, with
  its momentum, 125000). The blast is its destroyer's: blown up by a player, it hurts
  the enemies, and the kills are his (as "destroyed" in the kill feed); by an enemy,
  it hurts the players (Escalation's players never hurt themselves or each other).
- **Online**: the players' rounds, blasts and blows strike it on every PC, as they
  strike the enemies there; the PC whose player (or, the host's, whose AI) blows it
  up says so, and it blows up on the other PCs too, as that player's or the enemies'.

Each match starts with it standing. `tools\build_barrels.py` reads the barrel's
placement and its blueprint (its states' meshes, effects, health, damage modifiers,
blast and debris) from `ESC_Ark_ART_m.xxx`; its debris are mesh effects of
`tools\build_weapon_fx.py`'s (`Debris1_STAT_FX` ..., their meshes from the same
export of `ESC_Ark_ART_m.xxx`, beside its `all_shared` one):

```
assets\models\props\exploding_barrel.glb   the barrel, its outline and its wreck (0.35 MB)
assets\models\props\exploding_barrel\     their textures (4 KTX2 files)
assets\levels\oblivion_barrels.txt          where it stands and what its blueprint does
assets\levels\oblivion_barrels.bin          its collision, standing and wrecked
```

```
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Ark_ART_m.xxx
cd Documents\foc_esc\tools
python build_barrels.py <export> "<CookedPC>" ..\assets
```

### The Ancients map

Ancients (`ESC_Temple`, the game's map 1602: a rusting canyon strewn with the wrecks
of giant Guardian robots):

- **its terrain**, the canyon floor with its toxic pits and rock cliffs. umodel can't
  export a terrain, so `tools\ue3\terrain.py` reads it from `ESC_Temple_ART_m.xxx`:
  101 x 117 heights 2.56 m apart, in four parts. The game draws it smooth: near the
  camera each quad of the grid is cut 4 x 4, the heights in between from the
  Catmull-Rom spline through the grid's (its `FPatchSampler`). The demo's mesh
  keeps to that surface within 2 cm, cutting each quad 1, 2 or 4 times as its shape
  needs (83,000 triangles), with the spline's own normals. Its three layers (rusty
  floor plates, plain rust and cliff rock: `ENV_RST_RustyFloorPannel2`,
  `plainrust3ti3_CLR_Mat` and `RS_cliffwall_CLR_Mat`, each repeated over the terrain
  as the game maps it) are painted by the game's own weight maps: the first is drawn
  opaque and the others over it, see-through by their weights (the vertex colours),
  in that order (glTF material extras `"depthbias:<n>"`, which `src\lighting.rs`
  applies). Each part has its own light map, four texels a quad, decoded with the
  rest. It collides as it is drawn: the game's terrain collides at its
  `CollisionTesselationLevel`, which is 4 here;
- **every area open**, as the level stands once each console has been bought: the
  Matinees the consoles' purchases play, to their ends (the bars of pillars drop
  into the floor and ceiling, the three dropship doors lift away, a pair of sliding
  doors slides aside, the retractable cover folds away), and the eight big Optimus
  doors the consoles open (`TnSeqAct_TransformWorldProp`, which `kismet.py` reads
  too) transformed, at the end of their opening (`AP_OptimusDoor_Open`). The five
  that stay shut are at the map's edges (three open only for the enemies' waves),
  four of them under faint orange force fields;
- its consoles, loot pods and Optimus doors, posed as the level leaves them;
- **the sky**: a grey dome (`greytest`) lit by the sun, under the level's three
  layers of height fog (both packages': a bright, warm one far out, from 200 m; the
  pale one over the play area, from 20 m; and a green one in the toxic pits, which
  views of the sky never reach), baked into the dome as on Downfall. Over the map
  itself the haze is the pale layer alone, as one exponential haze (0.6% per metre,
  matching the game's at 60 m, as Bevy's haze starts at the camera); the pits' green
  layer and the far layer are left out of it. The sun's light shafts (its bloom
  scale 10) wash what stands round it in a warm glare (*The sun's light shafts*);
- the toxic sludge glowing in its pits (`ENV_RST_ToxicSludge_MAT`, added on), the
  floor debris's painted shadows, the cage lights' blue glow, the wrecked Guardians'
  glowing hands and heads, and the rocks' and debris's own materials (the cliffs'
  rock repeated 5 x 10 and 2.5 x 5 over them, as the game's materials tile it).

```
assets\models\ancients.glb          the map: BSP + terrain + 1,166 placed meshes (doors and glow spheres included)
assets\models\ancients\*.ktx2       its 214 textures (147 MB), block-compressed with mip maps
assets\levels\ancients_col.bin      collision: 876,607 triangles, + 524 for the blocking volumes and 708 for the player-only ones
assets\levels\ancients_points.txt   start point, level camera, KillZ, 88 enemy spawn points, 823 path nodes, 3 health stations, 3 ammo stations
assets\levels\ancients_lightmaps\   the game's baked lighting: 3 light-map pages (the terrain's included)
assets\levels\ancients_lights.txt   the sun, its light shafts, 57 point lights and a spotlight, haze, bloom (read by src\lighting.rs)
assets\levels\ancients_grade.bin    the map's colour grade (templeCLUT)
assets\levels\ancients_fx.txt       its effects: 10 materials, 3 particle systems placed 6 times, a lens flare placed 16 times
assets\menu\img\lobby_map_ancients.png   its picture in the lobby (made by build_menu.py)
```

To rebuild it, export the two packages whole, the covers' animation on its own and
the two textures the pickup bases take from a shared package, all into one folder,
`<export>` (the terrain needs no export: `build_map.py` reads it, and its light maps,
from the packages):

```
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Temple_ART_m.xxx
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Temple_BASE_m.xxx
umodel -game=trans -path="<CookedPC>" -export -out=<export> ESC_Temple_BASE_m.xxx AP_CoverABIG_ANIM AnimSet
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Color_NormX Texture2D
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Masks_NormY Texture2D
cd Documents\foc_esc\tools
python build_map.py ancients <export> "<CookedPC>" ..\assets
```

### The Ignition map

Ignition (`ESC_Gladiator`, the game's map 1601: a Decepticon arena in Kaon, dark halls
of steel lit by green and blue light bars, with a great statue of Starscream):

- **every area open**, as the level stands once each console has been bought: the
  level's green force-field gates (cards of `ForceField_ESC_GREEN_Matinst` that the
  consoles' Matinees lower into the floor or raise into the ceiling) where their
  Matinees leave them, and its 24 big Optimus doors open (20 that the consoles
  transform, `TnSeqAct_TransformWorldProp`, and 4 that start open);
- its consoles, loot pods and Optimus doors, posed as the level leaves them; the
  four steam vents in the floor and the Autobot dropship that flies past the
  arena, where their Matinees leave them (the dropship 390 m out, past the map's
  edge; their Matinee groups name their animation sets, `AP_SteamVent_ANIM` and
  `VIG_Dropship_AP_ANIM`, which `anim.py` reads from the package);
- **its glows as its materials make them**: the green light bars
  (`GreenLightBar_MATINST`: the glow mask x (0.47, 1, 0.3) x 8), the blue wall lights
  and pillars, the consoles' amber and the loot pods' cyan, from each material's own
  `Emissive_Color` and `EmissiveIntensity` (`tools\ue3\matparams.py`, as on every
  map: see "The Escalation maps");
- **the sky and its two moons**: a dome of `IaconSunset3_CLR` with its stars
  (`Spacedome_ashsunset_matINST`), banks of bright cloud 2.7 km across
  (`rs_skydome_STAT`, x 30), cards of stars and of cloud (`starmap_CLR_Mat`,
  `vint_clouds_CLR_Mat`) and **the two moons** low over the arena's west side
  (`cybermoon_CLR_Mat` on two cards nearly 600 m out, 96 m and 36 m across, x 10),
  each wrapped in **glow spheres**, the game's halo round them (`GlowSphere_STAT`
  with `MonitorGlow_MAT_INST`, drawn with its game material: a pale blue glow,
  brightest where a sphere faces the camera; three round the big moon, the largest
  360 m across, and four round the small one). All of it is seen through the
  level's height fog as the game lays it, which `build_map.py` works out for each
  vertex from the play area (`ray_fog`), as the game works out its fog for a
  see-through material at each vertex: the level's post-process volume's pale green
  fog fills the air up to 117 m, its `HeightFog`'s dark green one the air above
  that up to 616 m, from 350 m out, both at 0.3 of their densities (the level's
  `FOG_DensityWorldScale`, which only Ignition sets below 1). The fog hides most of
  the sky in its dark green, the clouds and stars a faint shading in it, while the
  moons, nearer and above the pale layer, show through it pale, in their bluish
  halo, as in the game. (UE3's layered height fog, as Fall of Cybertron sets it
  up, `FSceneRenderer::InitFogConstants` and its shaders: the layers, highest
  first, each fill the slab from the next one's top to their own; a layer thins the
  light by 1 - its density for each unit of the ray past its start distance, times
  the part of the ray in its slab; the light it lays over what it hides is 1 / ln 2
  of its colour x brightness. Before, `build_map.py` took each layer down to the
  ground and at its full density, which hid the moons and the whole sky.) The cloud
  cards fade out to their edges by their alpha, as the game's base pass multiplies
  what an additive material adds by its opacity (taken whole, they stood out as
  bright panels with straight edges). Over the map the fog is the volume's alone:
  one exponential haze, 0.45% a metre, a pale green grey (it was 1.5%, without the
  world scale). And as in the game, the level's depth of field softens it all (see
  "The maps' depth of field"): everything past about 190 m is 0.8 out of focus, the
  sky 36% sharp and 64% blurred, so the moons' edges soften, their light spreads
  into their halo and the stars and clouds blur into the haze;
- **the green force fields over the enemies' spawn doors and the windows** of their
  rooms (`ForceField_ESC_GREEN_Matinst`, `ENV_ForceField_ALL_MAT`), drawn with their
  game material (`src\levelfx.rs`): see-through, the room behind showing (or the black
  card the level hangs behind the ones that lead nowhere, `Black_Mat`), the circuit
  pattern drifting slowly down under bright bands that sweep across it, glowing where
  the field meets the walls and floor round it and more as it is seen edge-on
  (Ancients' and Oblivion's orange ones, `ForceField_ESC_Matinst`, the same); the
  dropship's shield and the steam vents' covers from their own materials;
- the great block of steel the arena is sunk into, plated to the top: the game
  leaves its top, its outer sides and its underside in the editor's grid
  (`BSP_Base`, `TEST_Grid_Materials_p.Grid_Grey`), which `build_map.py` gives the
  plating the arena's walls have up to the top (`Panel_02_TEXSET`);
- **a lid over the arena** at the block's top, 41 m up, for the players' movement
  only: the game puts nothing over Ignition (Downfall and Ancients have a blocking
  volume over theirs), so a jet could climb out of the pit and fly away over the
  block; here it stops at the top. The enemies pass it as they would the open sky
  (a Leaper's leap rises over it);
- **the gold statue of Starscream** over the arena (`TR_Starscream_*_MAT`, on the
  destructible statue `DES_KON_StarscreamStatue_p`), gold as the game shades it: its
  materials are the game's ENV_BRDF_A_MAT, the characters' shading, with a gold
  reflection coefficient (`CoefficientColor_C` (2, 1, 0.11)), `MetallicOffset` 0.69,
  a dim diffuse (`DiffuseIntensityScalar` 0.16) and highlights of power 300, so it
  shines in the arena's lights and reflects the level's captured cube map like
  polished gold. `build_map.py` marks such materials (a coefficient past 1.2: only
  the statue's, on any map) with their values (`brdf:` in their extras), and
  `src\character.rs` draws them with the characters' paint shader
  (`assets\shaders\paint.wgsl`), unpainted.

Its effects (see "The maps' effects"): the 49 light shafts (`LightBeam_Cone_STAT`)
under its lamps, pale blue, warm and cool; its pin lights and glow spheres, the moons'
seven among them (seen through the sky's fog, above); its 12 steam vents, 12 wisps
of steam and 8 fires; and 146 lens flares on its lights. Left out: a glow sphere
1.2 km across around the whole level, whose mesh names no material.

```
assets\models\ignition.glb          the map: BSP + 2,756 placed meshes (doors, sky, statue and effect meshes included)
assets\models\ignition\*.ktx2       its 301 textures (177 MB), block-compressed with mip maps
assets\levels\ignition_col.bin      collision: 675,041 triangles, + 808 for the blocking volumes, 434 for the player-only ones and the lid
assets\levels\ignition_points.txt   start point, level camera, KillZ, 130 enemy spawn points (8 of them far outside the map, where the Leapers leap in from), 981 path nodes, 4 health stations, 4 ammo stations
assets\levels\ignition_lightmaps\   the game's baked lighting: 32 light-map pages and the vertex-light page
assets\levels\ignition_lights.txt   the sun, its light shafts, 191 point lights and 45 spotlights, haze, bloom, depth of field (read by src\lighting.rs)
assets\levels\ignition_grade.bin    the map's colour grade (L05_Soundwave_CLUT)
assets\levels\ignition_fx.txt       its effects: 14 materials, 3 particle systems placed 32 times, 5 lens flares placed 146 times
assets\menu\img\lobby_map_ignition.png   its picture in the lobby (made by build_menu.py)
```

To rebuild it, export the two packages whole and the two textures the pickup bases
take from a shared package, all into one folder, `<export>` (the steam vents' and the
dropship's animations need no export: `anim.py` reads them from the package):

```
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Gladiator_ART_m.xxx
umodel -game=trans -path="<CookedPC>" -export -gltf -png -out=<export> ESC_Gladiator_BASE_m.xxx
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Color_NormX Texture2D
umodel -game=trans -path="<CookedPC>" -export -png -out=<export> ENV_MP_PropsShared_p.xxx MP_PickupBase_TEXSET_Masks_NormY Texture2D
cd Documents\foc_esc\tools
python build_map.py ignition <export> "<CookedPC>" ..\assets
```

### The maps' lighting

The game lights its maps mostly with light baked by Unreal's Lightmass, and the
demo uses that same light (`src\lighting.rs`). Downfall's, in detail:

- **Light maps.** The level package says which part of which light-map page each
  mesh and each patch of BSP uses; the pages themselves (DXT1, zlib-compressed) sit
  in `ESC_Iacon_ART_m_LM.xxx` (and the gameplay package's own, for its props).
  Each page pair stores, per texel, how much light arrives from three directions
  around the surface and its colour, gamma-encoded against the light map's own
  brightest value, with a gamma that grows with that value (by its log2, past 2) so
  that 1.0 is always stored alike: decoded that way, the light maps of neighbouring
  surfaces, each with its own range, meet at their edges (Ancients' terrain parts
  showed it). `tools\ue3\lighting.py` turns them into plain light (the light a
  surface gets along its normal) in PNG pages, and
  `build_map.py` gives every mesh node a glTF `extras` entry naming its page and
  rectangle, which `lighting.rs` turns into Bevy `Lightmap`s. Props the game lights
  per vertex get their light as vertex colours. Texels hold light / 4
  (`LIGHTMAP_RANGE`), so light stronger than 4x white saturates. The pages hold the
  stock UE3 sum (each direction's light times its cosine to the normal); the game's
  own light-mapped shaders (every one in its compiled shader caches) weigh each
  direction by (1.127 x that cosine) squared instead, which on a surface facing along
  its normal comes to 0.733 of it, and `lighting.rs` gives the light maps that
  (`LIGHTMAP_FLAT`). Props the game lights live (the consoles, loot pods and Optimus
  doors among them) get the light the demo gives Jazz.
- **The sun**, the level's `DominantDirectionalLight` (a low, pink sunset sun,
  brightness 6), is the one light the game computes live; here it is a directional
  light with cascaded shadows, so it only reaches what's open to the sky. Nothing
  see-through casts a shadow from it, as in the game (its translucent materials
  cast none): not the skies (`noshadow`), nor what is drawn added on or blended
  (`lighting.rs`'s `map_material_flags`). The layers added over a sky dome (its
  haze, Downfall's sun glow) surround the map just as the dome does, and Bevy's
  shadow pass drew them as solid, so Downfall's, Ancients' and Ignition's suns lit
  nothing at all: their shadows started at the sky and covered every map.
- **The sun's light shafts** (`src\lightshafts.rs`, below).
- **The lights** (81 point lights) are already in the light maps; as in the game,
  they also light the moving things (Jazz, the enemies) and add highlights to the
  map. They are matched to the game's brightness a quarter of the way out along
  their radius.
- **Post-processing** from the level's post-process volume: exponential haze (its
  three height-fog layers as one: 0.35% per metre, a dusky mauve), a mild bloom of
  anything brighter than white, and its colour grade (`VortexFlyingPrevis_CLUT`),
  applied after the game's tone curve: the game's default post-process chain
  (`ENV_PostProcess_p.Trans_DefaultScenePostProcess`, which no level replaces) maps
  light with John Hable's filmic curve (`TMM_FilmicSCurve`, white at 11.2), as the
  Customize garage does. Bevy's TonyMcMapface tone-mapping table is replaced with
  the two together.
- **What the characters reflect**: their metal reflects the cube map the level
  captured (its `StaticSceneCaptureCubeMapActor`'s), or where it has none, as
  Downfall and Oblivion have none, their material's default (`rommCubeMap`, from
  `all_shared.xxx`): the lighting file's `reflect` line names it
  (`assets\textures\levelfx\<map>_cube.ktx2`, or `romm_cube.ktx2`), and
  `build_map.py` writes both.

The level materials are not metallic (the game's shading never dims the diffuse
light for shininess, and the light maps assume full diffuse; Ignition's gold statue,
shaded as the characters are, is the exception), and the game
camera, HUD and menu cameras are HDR (needed for bloom; cameras sharing a window
must agree). A map without its `levels\<map>_lights.txt` gets the old sun, fill and
ambient lighting instead.

Units: the game's brightness 1.0 is "white"; Bevy's default exposure (EV100 9.7)
divides light by about 1000, so game values are multiplied by `1 / exposure`
(times pi for lights, as Bevy's diffuse divides by pi).

### The sun's light shafts

Three maps have a light that draws the game's light shafts (its `bRenderLightShafts`):
the rays of the sun streaming past whatever stands in front of it, and the glow
round it. `build_map.py` writes that light as a `shafts` line in the map's lighting
file (`tools\ue3\lighting.py`'s `shaft_light`), and `src\lightshafts.rs` draws them
as the game does:

| map | the light | BloomScale | BloomTint | occlusion range | screen blend |
|-----|-----------|------------|-----------|-----------------|--------------|
| Downfall | `DirectionalLight_13872` (it lights nothing: its lighting channels are all off; it shines the way the sun does) | 1 | 255, 199, 184 | 1 km | 0.2 |
| Ancients | its sun | 10 | 255, 235, 189 | 200 m | 1 |
| Ignition | its sun | 0.5 | white | 200 m | 1 |

(Oblivion has none. Downfall's light sits in `ESC_Iacon_ART_m`'s
`StaticLightCollectionActor`, placed by the matrix the actor keeps for it after its
properties.) The game's
shaders, read from its global shader cache (`Engine.xxx`'s
`TDownsampleLightShaftsPixelShader<0>`, `FBlurLightShaftsPixelShader` and
`FApplyLightShaftsPixelShader`), run on the picture after the see-through things,
before the depth of field, bloom and tone curve:

- **Downsample** to a quarter of the size: what is near the camera blocks the light
  (its depth over the light's `OcclusionDepthRange`; the screen's edges never do),
  and the picture's light over `BloomThreshold` (0) times `BloomScale`, from the
  far half of that range only (the sky), within a fifth of a screen height of the
  sun's place on the screen, is the shafts' light.
- **Blur** it toward the sun: 48 samples from each texel to the sun's place (or
  part of the way, from more than half a screen height off), weighted from 2 down to
  nearly 0.
- **Apply**: the picture is darkened where the way to the sun is blocked (down to
  `OcclusionMaskDarkness`, 0.3) and brightened where it is open (to 1.35), less of it
  further from the sun (none two screen heights away) and as the sun leaves the
  screen; and the shafts' light, tinted by `BloomTint`, is added four times over,
  less over what is already bright (`BloomScreenBlendThreshold`).

So looking toward Downfall's sunset, the towers and gantries against it go dark
and the rays stream between them; Ancients' sun, with its strong bloom (10), washes
the rocks and scaffolds round it in a warm glare, as in the game. They come with the
bloom: the Graphics levels without it (Low) draw none.

### The maps' depth of field

A level's post-process volume can turn on UE3's depth of field (`bEnableDOF`), and
three of the Escalation maps' do, each for the far distance only (no near blur);
Ancients' doesn't (its BASE package's WorldInfo does, but inside the volume the
volume's settings are the ones the game uses):

| map       | in focus to | blurred over | at most | blur kernel |
|-----------|-------------|--------------|---------|-------------|
| Ignition  | 0           | 200 m        | 0.8     | 32 pixels   |
| Downfall  | 0           | 2 km         | 0.5     | 16 pixels   |
| Oblivion  | 500 m       | 200 m        | 0.4     | 16 pixels   |
| Ancients  | -           | -            | off     | -           |

`src\dof.rs` and `assets\shaders\dof.wgsl` draw it as the game does, from its
compiled shaders (`Engine.xxx`'s global shader cache: `TDOFGatherPixelShader<16>`,
`FUberHalfResPixelShader<0>`, the uber post-process) and TFOC.exe's blur set-up:

- how out of focus a depth is: min((|depth - focus| / falloff)^4, at most), with
  the near or the far falloff and limit (Fall of Cybertron's own
  `DOF_FocusNearFalloff` and `DOF_FocusFarFalloff`, in place of UE3's single focus
  radius), so on Ignition 6% at 100 m, 32% at 150 m and 0.8 from 189 m on;
- the picture cut to a quarter of its size each way, each texel the mean of the 16
  pixels it covers, weighted by how out of focus their mean depth is;
- that blurred across and down by a Gaussian whose variance is its radius:
  min(kernel / 4, 16) x the screen's width / 1280 texels, at most 15 (12 texels, so
  a spread of about 14 pixels, at 1920 wide on Ignition; 6 on the other two);
- the picture blended toward the blur by how out of focus each pixel is, with what
  is in focus kept out of the blur (picture x f + (1 - f) x (blur + picture x f) /
  (blur's weight + f), f = 1 - out of focus): where everything round is equally far
  out of focus, the picture moves toward the blur by its square.

It runs on the game camera before bloom and tone mapping (and the pause menu's
hologram), as the game blends it before adding its bloom. `tools\build_level.py`
writes the volume's settings as the lights file's `dof` line (focus, near and far
falloffs in metres, exponent, most near and far blur, kernel; `dof_line`), which
`src\lighting.rs` reads.

### How the maps are drawn

A map places its meshes thousands of times (2,200 to 4,200 parts), nearly every one
with its own rectangle on a light-map page. Bevy gives every light-mapped thing a
slot of its own in its light-map bind groups (four to a group) and makes every one of
those bind groups again, in every render phase, every frame, and each part was a
draw of its own too: on the maps that kept the frame rate down to around 40 even on a
fast PC, one CPU thread busy and the graphics card half idle. So once a map is in,
`src\merge.rs`:

- puts its light-map pages onto one atlas, up to 49 pages to an atlas (the maps have
  3 to 33; each page with a border of its edge texels repeated, two wide, so
  filtering never reaches into its neighbour);
- joins the parts that draw alike (one material, one atlas, the same vertex layout
  and shadow settings) into one mesh each, in the map's own space, each part's
  rectangle on the atlas moved into its second UV set. A mirrored part (about a
  quarter of them are placed with a negative scale) has from Bevy's glTF loader a
  copy of its material that culls the other side; it is joined with its material's
  unmirrored parts instead, its triangles turned round. Downfall's 2,200 joined
  parts become 119 meshes, Oblivion's 4,100 115, Ignition's 2,600 85 and Ancients'
  1,100 61. See-through parts (drawn back to front, part by part) and the effect
  meshes (below) stay as they are, their light maps moved onto the atlas.

The map is hidden as it comes in and shown once that is done, a moment later (Bevy
hands out its light-map slots as things first show, and never gives them back).
The joined meshes are bigger, so less of a map is left out of a view as being off
screen: the graphics card draws more, which a modern card has room for.

### The maps' effects

Much of the maps' look comes from the game's effects: soft shafts of light under
the lamps, glowing spheres, cards of smoke and fog, pin lights, smoke columns, steam
vents, fires and lens flares on the lights. `tools\levelfx.py` (run by
`build_map.py`) takes them from the map's packages and `src\levelfx.rs` draws them
as the game does:

- **With the game's own materials.** An effect material is an Unreal material
  graph: texture samples, panning and turning coordinates, falloffs by the angle of
  view, fades where the effect meets the scene, colour curves, High Moon's own HLSL
  snippets and material functions. `tools\ue3\fxmat.py` follows each material
  instance to its base material and compiles the graph, with the instance's
  parameters and static switches, into a WGSL function; `levelfx.py` puts every
  built map's materials into `assets\shaders\levelfx.wgsl` (from
  `tools\levelfx_shader.wgsl` and `assets\shaders\levelfx\<map>.wgsl`), each picked
  by a shader def, and writes their textures to `assets\textures\levelfx\` (KTX2
  with mip maps; the little colour ramps as PNG), each under its own name, or, where
  the game's packages have two different textures of one name among the effects
  built together, the second also under its path
  (`Streak_CLR__FX_OS_Common_Textures_p_Textures.ktx2`). They blend as the game's do
  (added, as much as their opacity says; laid over; or multiplying the picture),
  lit ones (the smoke) take the light of the sun and the map's lights around them,
  and the camera's haze covers them. The map's lights reach them as the game's
  particle light environments give it: gathered, where the effect starts, into one
  light from the way most of their light comes, falling on the smoke by how squarely
  it faces that way (as the lights fall on the characters), the rest of it as light
  from all around at its average, a quarter (`light_at` in `src\levelfx.rs`). (Every
  light's light used to be added whole from all around, smoke facing away from a
  light lit as if facing it, some 20 times the light from all around beside the
  lights the maps put there for the characters.) Effects the game spawns (weapons'
  flashes, impacts and blasts, the emitter pool's) take no light at all, lit
  materials or not: the engine's particle components accept no lights
  (`bAcceptsLights` is false by default and neither `ParticleSystemComponent` nor
  `EmitterPool`'s template turns it on) and have no light environment, which only
  some of the maps' placed emitters are given (9 of Oblivion's 45, 20 of Ignition's
  32: each with both). So their lit smoke shows its own emissive colour alone: a
  tank shell's blast is its orange fire in a dark, see-through haze that darkens
  what is behind it, as in the game, where the demo's was a cloud of grey or white
  smoke as bright as the lights around it (`spawned_material`). A destructible's
  chunks are not particles in the game but rigid bodies with light environments of
  their own, so the ones in lit materials (Oblivion's barrel's, the Swarmer's and
  Downfall's scaffolds' pieces) are lit by the light where they are thrown from, their
  highlights too (`src\gibs.rs`; `tools\levelfx.py`'s `lit_solid`): drawn by their
  emissive colour alone, the scaffolds' steel beams were black. The explosions a
  tank's shell makes of the enemies it kills (their deaths' `Exp_Fireball_FX` and
  `Exp_Flak_Lrg_FX`) end in lit smoke the game draws through particle G-buffers of
  its own (`FinalLitSmoke`: `LightSmokeFromGBuffer1_MAT`, its scene reads
  `SceneTex_GBuffer1` and `SceneTex_GBuffer2`, which the smoke is drawn into and read
  back from). The demo has no such buffers, so that smoke is left out
  (`tools\ue3\fxmat.py`: a read of one finds it empty, and a material drawing what
  they hold is dropped as the blur buffer's are); drawn reading the picture instead,
  its 30 m sprites turned whatever was dark behind them black for their 13 seconds,
  the background going black after a few tank kills. The fire that hides behind that
  smoke (`OccludedHeat_MAT`) shows whole. The maps' placed
  emitters are lit only where the game lights them, as each placement's line in
  `levels\<map>_fx.txt` says (its last field, from its component's `bAcceptsLights`
  and `LightEnvironment`, `tools\levelfx.py`): Downfall's burning pillars and
  ash clouds, Ignition's fires and the steam along its floor, Oblivion's reload steam
  by the Energon Harvester and Ancients' fire are; the steam vents of Oblivion,
  Ignition, Ancients and the Customize garage and Ancients' rust storm are not (their
  materials are unlit anyway). The traps' flames are lit too, as the game lights
  them (`levels\<map>_doors.txt`). An effect's emitters
  are drawn one after another in its order, as the game draws a particle system's
  (`EMITTER_ORDER`): the blast's fire over its smoke, not behind it.
- **Each material its own shader.** A material's pipelines don't compile the whole
  of `levelfx.wgsl` (every map's, the garage's and the weapons' materials, 650 KB):
  Bevy works through the whole of a shader's file for each pipeline it makes, one
  pipeline at a time, and with an effect pipeline for each material, part and blend
  that took about 60 ms each and held everything else up as the demo started (see
  *What's new*). As the demo starts `split_shaders` (`src\levelfx.rs`) reads the
  file and makes each material a shader of its own in memory: the code they share
  (comments left out) with only that material's function, some 18 KB. Its
  pipelines use it (`use_material_shader`); the file on disk is the same as before,
  and a material without a shader of its own would use the whole file as before.
  The shaders' results are the same: all 655 of their variants checked here compile
  to the same code from either.
- **Effect meshes** (`LightBeam_Cone_STAT` light shafts, glow spheres, smoke cards,
  fog planes, pin lights, and wherever they are, the materials `build_map.py`'s
  `FX_MATERIALS` lists: Oblivion's windows, its space view, energy fields and
  animated signs, Downfall's electric spawn gates) are placed in the map's `.glb` with the other meshes, with a
  stand-in material whose extras (`fx:<n>`) tell `levelfx.rs` which effect material
  draws them. The far backdrop (Oblivion's space) is drawn outside the haze; an
  effect mesh far out in a map's sky (over 300 m from the play area: Ignition's
  moons' glow spheres) is seen through the fog the sky is seen through, worked out
  for each of its vertices (`build_map.py`'s `ray_fog`, in its vertex colours, extras
  `skyfog`: `FX_SKYFOG`) rather than through the camera's haze; and an
  effect that reflects a cube map (Oblivion's tower window) reflects the one the
  map's characters do. A flip book (`TextureFlipBook`) shows its frames in turn at
  its own rate (or picked at random, as it says).
- **Particles.** High Moon cooked each emitter's modules into a compact form of its
  own; `tools\ue3\particles.py` reads it (lifetimes, sizes, velocities, colours over
  life, sub-images, dynamic parameters, cylinder and sphere start shapes, attractor
  points, High Moon's gravity, spawning by distance moved, particles born where
  another emitter's are...) and `levelfx.rs` runs the modules as Unreal's
  emitters do, every frame: particles' speed, size, spin and colour back to their
  base values, the update modules, new particles spawned by the spawn rate and
  bursts (each through the spawn modules), then movement. (An emitter with no
  `SpawnRate` of its own has the engine's default, which `Engine.xxx`'s
  `Default__ParticleModuleRequired` gives as a constant 0: it spawns only its
  bursts.) Each emitter's pieces that are done are hidden at once and despawned
  three frames later (`Retired`): despawned in the frame other effects' pieces
  came, Bevy could draw one once more in its own material with a newcomer's mesh. An emitter whose
  particles never die (it gives them no lifetime, or one of 0, as the glow meshes
  in a jet's hover jets and afterburners and in a tank's boosters do) has no more of
  them out than the game's data has it keep (its cooked `PeakActiveParticles`: one,
  for those), spawning no more while they last, and they go when its effect is
  turned off. Each particle is drawn as a quad facing the camera (or stretched along
  its velocity, or lying flat, as its emitter says) showing its sub-image, sorted as
  the emitter sorts them. A particle locked to an axis keeps to the effect's own
  axes when its emitter is in the effect's space, and to the world's when it is in
  the world's (UE3's `GetAxisLockValues`): the dust sheet of a car's landing
  (`Vehland_*_FX`, locked to Z) lies flat on the ground however the socket it comes
  from is turned. (It used to follow the socket's turn: Bumblebee's car, whose
  landing socket is pitched up 30 degrees, threw up a wall of dust 40 m across that
  filled the screen.) The emitters the level starts with running are drawn,
  warmed up as though they had been running since the level loaded (Downfall's
  giant smoke columns for the 30 s their system asks for). An emitter with no
  particles out leaves its mesh as it is (a changed mesh is sent to the graphics
  card again).
- **Lens flares** (`LensFlareSource`): each element of a flare is drawn on the line
  from its light through the middle of the screen, sized, coloured and faded with
  the light's distance by its curves; the flare materials brighten the picture
  behind them. A light hidden behind the level (a ray to it hits the collision)
  shows none; the rays go to the lights on screen, a few a frame (a light coming
  into view is tested first), and the flares fade in and out over a few frames.
- **Soft edges.** The effects fade where they meet walls and floors (the game's
  `DepthBiasedAlpha`), which needs the scene's depth: the game camera has a depth
  prepass. The materials that bend or brighten the picture behind them (the fires'
  heat, the flares) read a copy of that picture, which Bevy keeps while anything
  see-through of that kind is drawn (`levelfx.rs` draws an invisible one).

| Map | Effect meshes | Particle emitters | Lens flares |
|---|---|---|---|
| Downfall | 19: glare and blue glow spheres, warm smoke cards, the oil fire's flame card | 11: 3 giant burning smoke columns, 8 smoke pillars | 55 |
| Oblivion | 73: 27 window panes, the energon holding tower's window, 30 red energy fields, 10 animated energon signs, the space dome, the space bridge and the vortex's three parts | 26: 22 steam vents, 4 harvester steam vents (`ReloadSteam_Harvester_FX`) | 96 |
| Ancients | 7 glow spheres | 6: 4 steam vents, a fire, the rust storm | 16 |
| Ignition | 69: 49 light shafts, 12 pin lights, 8 glow spheres | 32: 12 steam vents, 12 wisps of steam, 8 fires | 146 |

Left out: the emitters Kismet starts later in a match (Ancients' electric fences, a
trap the demo hasn't got: Ignition's flame jets and Oblivion's incinerator are the
traps', *Escalation's traps*; and Oblivion's anti-air
guns, its other reload-steam vents (`ReloadSteam_Tech_FX`), which the level switches
off as it starts, and the laser shots its opening Matinee fires), beam and mesh
emitters, and High Moon's wind module (the levels have no wind source). The maps'
emitters always use their most detailed level of detail (the weapons' effects pick
theirs by distance: *The Riot Cannon*). Flares are hidden by a ray against
the level's collision, not by the game's occlusion queries.

### Escalation's shop

Each map's purchase points are the game's (`TnPickupFactoryPurchasable` in the map's
`_BASE_m` package): `tools\build_map.py` writes them into the map's points file and
`src\shop.rs` puts them in play (with `src\main.rs`'s health and ammo stations).

- **Weapon stations** (the factories' `PPT_Weapon` and `PPT_HeavyWeapon` presets): the
  weapon floats still over the base the map has there, 2 m up, held as the game holds
  it (its Pickup socket at the factory, the barrel along the factory's -X), as the
  game's pickup (`TnWeaponMesh.GetValidPickupMesh`): the gun's mesh in its reference
  pose at its `PickupScale` (1), its forearm cuff with it, the gun in its
  `PickupMaterialOverride` (a `PICKUP_*_MATINST` of the characters' material: painted
  grey, 0.396, metallic 0.4) and the cuff in its own (`RB_ArmUp_Cust_MatINST_INST`),
  both drawn by the characters' paint shader (`character.rs`'s `paint_pickups`, from
  the "pickup_paint" extras `build_assets.py` gives their materials). It sells at
  the weapon's own Escalation price (`TransWeapons.ini`'s `PurchasePriceEsc`; the game
  ignores the factory's own cost for weapons) and is gone for the factory's respawn
  time once bought. The heavy weapons (`WeaponType` 2 in Escalation) are yellow: their
  outline and glow (the game's `OutlineColor` is 1, 0.396, 0, which its picture shows
  as a light yellow; `build_assets.py`'s HEAVY_GLOW and HEAVY_OUTLINE are what it
  shows), and their names on the purchase line; the others' are cyan. A weapon bought
  or picked up takes the place of the one in hand, whatever its type, as in the base
  game. A weapon you carry is sold again only while it is short of ammo, and fills it.
- **Ammo and heavy ammo crates** (`PPT_Ammo`, `PPT_HeavyAmmo`): the ammo crate fills
  the regular guns and the vehicles' guns, the heavy crate (the game's yellow one) the
  heavy weapons; each is offered only while a gun it fills is short.
- **Equipment stations** (`PPT_AssaultEquipment`, `PPT_UtilityEquipment`): the
  support item's pickup turns over the base, a metre up (the Diffraction Barrier and
  the Dimensional Decimator 1.5 m), at the factory's rate. The Diffraction Barrier's
  shield, the pickup mesh's see-through section, is drawn with its own material, the
  game's `MP_Barrier_Distortion_MATINST` (of `TR_Barrier_Distortion_AA_MAT`), as an
  effect turning with it (`build_weapon_fx.py`'s MESH_SYSTEMS). Bought, it goes into its slot (the Thermo Mine Payload
  the offensive one, on Assault T.E.C.H., key 1; the others the defensive one, on
  Utility T.E.C.H., key 2), in place of what was there, and the HUD shows it under the
  ammo count. The Dimensional Decimator's station on Oblivion is hidden all match by
  the map's Kismet in the game, so the demo leaves it out (build_map.py's
  HIDDEN_FACTORIES).

The pickups' models are built by `tools\build_pickups.py` (with `build_assets.py`'s
pickup looks): `assets\models\weapons\*.glb` with their sockets in
`assets\models\weapons\sockets.txt`, and the crates and support items in
`assets\models\props`. (The weapons' texture sets must be the full-size ones,
512 pixels, from `all_shared.xxx`: some level packages keep 128-pixel copies.)

The purchase points' bases (the map's `ESC_PickupBase_*` meshes and the health
stations' `health_base.glb`) are drawn with the game's `Pickup_Unlit_MAT` shading
(`src\pickupbase.rs`, `assets\shaders\pickup_base.wgsl`, from the game's material
graph): unlit, a light of its own circling the base (`(cos 1.5t, sin 1.5t, 1)`), its
colour half lit by it with a wrapped falloff, a highlight and the level's cube map
by its gloss mask (`SpecularColor` 0.95, 0.93, 0.88; the heavy weapons' bases'
0.56, 0.57, 0.58), and its lights the map's glow. The gloss mask is read back from
the textures' metal channel, where the tools keep it: 0.2 + 0.5 x the mask in the
maps' bases (`tools\build_map.py`), 0.25 + 0.55 x it in `health_base.glb`
(`tools\build_assets.py`).

The pickups' glowing outlines (`build_assets.py`'s `outline_shell`: the pickup's
triangles again, pushed out 2.5 cm along the smoothed normals and turned inside out,
in the `PickupOutline` or `PickupOutlineHeavy` material) are drawn by
`src\outline.rs` with `assets\shaders\pickup_outline.wgsl`, Bevy's mesh vertex
shader with each corner moved back along its line of sight to the back of the
pickup's bounding box as the camera sees it (from the shell's own bounds): the same
place on the screen, but behind every part of the pickup, so only its silhouette's
outline shows, as the game's does. What a pod makes goes back no further than 2 cm in
front of the pod's back wall (an `OutlineBackstop` plane on the pickup, 1 m behind
the pod's middle), which a weapon's back is in. They take no part in the depth prepass (their depth is their
own vertex shader's) and cast no shadows. Each shell is drawn by a copy of it in
that material, beside it, the shell itself hidden: giving the shell itself the new
material could leave it, for a frame, in the depth prepass with its old material's
pipeline, and wgpu stopped the game with a validation error.

### Escalation's support items

What the equipment stations sell is used with **1** (Assault T.E.C.H., the offensive
slot: the Thermo Mine Payload and the Dimensional Decimator) and **2** (Utility
T.E.C.H., the defensive slot: the Diffraction Barrier, the Energon Recharger and the
Intercooler), in robot form only, as the game's abilities run them (`src\equipment.rs`,
from `TransGame.xxx`, the Escalation map packages' `TransContent` and the game's
config):

- **The slots** (`TnAbilityManager`): each item has one use (its resource of 1 spent
  by it). Once used, its slot shows red with its icon at 90% while its effect lasts,
  then empties; its key does nothing for a second after a use (the game's spam guard).
  The barrier and the Decimator can be cancelled: their key again lets the barrier down
  or sets the ball off. Buying into a slot ends what its item had going (a standing
  barrier goes, a rolling ball goes off), but a buff runs on. Dying loses the items not
  yet used and ends what they had going.
- **Bought**, the item shows at the top right as the game's HUD shows it
  (`mc_itemUnlocked`): its icon flies in, its wireframe picture comes up and its name
  types in beside the icon, all in cyan, and it goes after 6 s.
- **Thermo Mine Payload**: "Thermo Mine Deployed" in the game's messages at the top
  left, and three mines let out half a second apart, about 9 m from you (to your left
  ahead, ahead, and to your right), pulled in against a wall in the way. Each falls to
  the floor, rises off it and bobs between 0.5 and 3.15 m over it, spinning, and once it
  has hovered 0.4 s it looks for an enemy within 65 m that it can see. Finding one, it
  plays its fuse sound, lets off steam and homes on it at 20 m/s, going off as it reaches
  it (or a wall): 250 damage to every enemy within 8.5 m, through walls, with the game's
  grenade blast at half size, its sound and a shake for players beyond its reach.
- **Diffraction Barrier**: the game's curved wall, 15.7 m wide and 8.4 m tall, 10 m
  ahead of where you look, growing in over 0.2 s, its edge and its sweeping band of
  hexagons in your side's colour (purple for the Decepticons, red for the Autobots: the
  game gives the wall its owner's energon colour), with its hum while it stands. It
  has 1000 health and wears away at 25 a second, so it stands 40 s; it goes with its
  retract sound. It stops the enemies' fire (the game's barrier is world geometry to
  their rounds: its physics asset's ten boxes): their bullets strike it, with their
  impacts on it, and their grenades bounce off it, and a grenade's blast doesn't reach
  you through it. What strikes it takes off its health (an Escalation barrier takes
  damage), so heavy fire brings it down sooner. Your own side's fire passes through,
  and the enemies can still see you through it.
- **Energon Recharger**: 15 s of healing, your health first and then your shield, at
  0.4 of your form's shield recharge (20 a second in robot form, 4.8 as a vehicle) and
  without the recharge's wait, with its sparks on your chest and its loop.
- **Intercooler**: 12 s, for you and every teammate within 25 m of you: 1.2 times your
  robot's speed and your class ability's meter refilling 8 times as fast, with its blast
  as you use it, its loop, and its buff's effect on everyone it gives it to
  (`Intercooler_FX`, `TnBuffFastAbilityCooldown`'s ApplyFx): lightning crackling over
  them, from their chests, for its 12 s. The blast (`Intercooler_Blast_FX`) goes off
  on your lower spine (the robot's `PositionSocket`, where the game plays it): its
  lightning at once, then at 0.4 s its sphere sweeping out from you, brightening and
  warping all behind it, and the ripple across the floor; the sphere is gone at 0.6 s,
  as its emitter ends (the game kills it then: `bKillOnCompleted`), as in the game's
  videos. Its big glow sprite (`GLOW`) is left out: the game's videos show no haze
  before the sphere, where it would wash the screen white from 0.1 s on.
- **Dimensional Decimator**: a ball rolled out 5 m ahead of you (25 m/s ahead and 5 up),
  rolling and bouncing, with its loop and its fuse sound; it goes off 2.5 s after it comes
  out, or as it strikes an enemy once armed (after a second), or as you press 1 again.
  It opens a **black hole** that rises (staying 5 m under a ceiling) for 5.5 s and kills
  every enemy within 50 m but the big ones (the Leapers, Enforcers, Bruisers and Titans),
  drawing their bodies in, then collapses, flinging them out.
- **Using any of them** plays the game's gesture (`Nav_ToStealth`) on your upper body,
  your legs going on as they were (`tools\build_robot_extras.py`'s
  `models\robot_extras.glb`).

Online, what each player uses happens on the other players' PCs too: the barrier, the
mines and the ball there, the Recharger's sparks on him, the Intercooler's lightning on
him and the teammates near him, and a teammate's Intercooler gives you its boost if you
are within its 25 m.

The mine's model and clips (`models\props\thermo_mine.glb`) and the ball
(`models\props\decimator_ball.glb`) come from `tools\build_pickups.py`; the effects
(the mines' blast and steam, the Recharger's sparks, the Intercooler's blast and
lightning, the Decimator's blast and black hole, and the barrier's three layers) from
`tools\build_weapon_fx.py`; the sounds from `tools\build_sounds.py`.

### Escalation's Armory Recreators

Every map has the game's **Armory Recreators**, the tall pods that make a random heavy
weapon for 250 energon (`src\recreator.rs`, from the maps' `TnInteractLootBox` actors
and the game's loot table): 8 on Downfall, 6 on Oblivion, 3 on Ancients and 7 on
Ignition, where the game has them.

- **Using one**: stand in front of it, facing it, in robot form (within 4 m of the spot
  5 m out from its door). The HUD says "Press E to Activate Armory Recreator *250" and
  shows your energon, in red when you have less than 250. Pressing E without enough
  plays the error beep and flashes the counter red. With enough, 250 is spent, the pod
  opens with its sound (`AP_StoragePodOpen`, 3 s) and the steam its vents let out
  (`ReloadSteam_Titan_FX`, from five vents in turn), and its cyan outline goes out.
  Each pod works once a match: a new match starts with every pod closed again.
- **What it makes**: one roll of the game's loot table (`ESC_LootTableBase`, the same on
  every map):

  | Item | Chance |
  |---|---|
  | Gear Shredder | 16.5% |
  | Thermo Rocket Cannon | 16.5% |
  | Energon Harvester | 16.5% |
  | Thermo Mine Payload | 16.5% |
  | Corrosive Slime Cannon | 10.7% |
  | A-4 Pulsar Cannon | 10.7% |
  | X18 Scrapmaker | 8.3% |
  | Chaos-Rift Combustor | 2.5% |
  | Dimensional Decimator | 1.7% |

  A weapon stands upright inside the pod, at the pod's weapon socket. A support item
  sits at the pod's other socket. After 2.5 s it can be taken from where you stood:
  "Hold E to pickup ..." gives you the weapon with its starting ammo, or puts the
  item in its slot. It waits there until someone takes it.
- **The outline**: cyan, round the pod's body, flaring to six times as bright every
  2.5 s while the pod can be used.

On Downfall, Ancients and Ignition the pods are the ones the map's model already has.
They are taken out of the map as it loads, whichever screen is up (a map loads behind
the menus at the start and during the lobby's countdown), and put back as pods that
open. Oblivion's
build leaves its pods out, so they are added with a box for their collision.

Online, the buyer's PC rolls the item and sends it with his Jazz, so every PC opens
that pod with the same thing in it. Whoever takes it, it is gone for everyone (the
network protocol is now version 27).

The pod's model, its opening and its materials (`models\storage_pod.glb` and
`models\storage_pod\`) and where each map has its pods (`levels\<map>_pods.txt`) come
from `tools\build_pods.py`, after Downfall's build. Not in: the opening's controller
rumble (the demo is played with keyboard and mouse).

### Escalation's locked doors

Every map has the game's **locked doors** and the **unlock consoles** that open them
(`src\doors.rs`, from the maps' `TnInteractCurrencyReceptacle` actors and Kismet): 11
consoles on Downfall, 11 on Oblivion, 10 on Ancients and 34 on Ignition, with the
doors, gates, Optimus doors, cover barriers, force fields and blockers they open, where
the game has them.

- **Using a console**: stand in front of it, facing it, in robot form (within 4 m of
  the spot 5 m out from its screen). The HUD shows the console's own line from the
  game, such as "Press E to deposit *250 of *1500 to open door" (the deposit in gold,
  the price in cyan), and your energon, in red when you have less than the deposit.
  Each press puts 250 in (500 at Ancients' bunker) with the deposit sound. Without
  enough you get the error beep and the counter flashes red. The two consoles on either
  side of a door share one total.
- **The console**: its screens play the game's animated flip book, its progress bar
  rises as its total grows (0.7 of the bar a second), and its outline glows cyan,
  flaring every 2.5 s like the pods'.
- **Paying the price**: the console plays its full sound, folds away (its
  `UnEquip_Console` animation) and its glow goes out. Then the map's own script for
  that purchase plays out, each step at its time: doors slide, rise or sink with their
  sounds, Optimus doors open (3 s), cover barriers fold down, Ignition's AirStrike
  station's force field collapses, player blockers go, the door's other consoles fold
  away, and the enemy spawn areas behind it open.
- **Collision**: a locked door blocks players, enemies and shots where it stands. An
  open one blocks where it ended up. The player blockers stop players only.
- **Oblivion's force fields** over its locked doorways (`ForcefieldPlane_STAT`, eight
  of them, with the force-field material `ForceField_ESC_Matinst`: a faint shimmer
  with orange bands running across) are drawn. Their mesh has one UV set, no tangents
  and no vertex colours, and the effect shader reads all of them, so they were left
  out, Bevy logging "Mesh is missing requested attribute: Vertex_Uv_1" (and, once it
  had a second UV set, "Vertex_Tangent") whenever one came into view. An effect mesh
  now gets what it lacks (`fill_fx_attributes` in `src\levelfx.rs`): a copy of its
  first UV set as its second, as Unreal reads a missing set, tangents worked out
  from its UVs, and white vertex colours, Unreal's for a mesh with none.
- **Downfall's scaffolds** (its 19 `TnStaticDestructibleActor`s, the game's
  `DES_ASH_ESCBeam1_DSYS` steel scaffolding) stand until a purchase brings them down
  (the Kismet's `SeqAct_TnActivateDestructible`: consoles 3800 and 6501 bring down
  15, 4242 and 6854 bring down 12, 9844 both sets). Each goes at once with the
  game's crash (`BL_DEST_MTL.DEST_ASH_COLUMN_EXPL`, from where it stands), and its
  collision goes with it; the same purchases open the enemies' navigation under them.
  It crumbles as the game's does (its broken state's
  `HmDebrisDestructionEffectFactory` chunks: `crumble` and `dsys` lines in
  `downfall_doors.txt`, thrown by `src\gibs.rs` as the deaths' gibs are): twelve
  beam parts each (`Beam1_pt1_STAT` to `Beam1_pt4_STAT`, 2, 2, 3 and 5 of them, in
  `ENV_beam1_matinst`, lit), at 1000 to 8000 uu/s in any direction from their
  places in the scaffold, spinning, for the chunks' ten seconds, and the
  `Catwalk_Debris_FX` burst of metal chunks. Every chunk and gib goes through the
  game's debris pool (`HmDebrisPool`: at most 50 out, the oldest removed to make
  room, at most five thrown a frame), and a long piece strikes the level with its
  thickness and comes to rest lying on its flat side.
- **A new match** starts with every door locked: the cover barriers up, every console
  at zero, and the enemy spawn areas behind the doors shut (the waves bring nobody in
  there until their door opens).

Online, every deposit is sent with the player's Jazz, so every PC has the same totals
and opens the same doors (the network protocol is now version 28).

The console's model, animations and materials (`models\esc_console.glb` and
`models\esc_console\`), each map's doors (`models\<map>_doors.glb`) and what its
purchases do (`levels\<map>_doors.txt` and `levels\<map>_doors.bin`) come from
`tools\build_doors.py` (with `tools\doors_kismet.py`, which reads each map's Kismet
and runs it offline), after the maps are built. It also takes the doors and consoles
out of the map's own model and collision (`models\<map>.glb` and
`levels\<map>_col.bin`, changed in place; running it again changes nothing more),
and Downfall's scaffolds with them (`collapse` lines in `downfall_doors.txt`):

```
python build_doors.py "<CookedPC>" ..\assets ancients <export> downfall <export> oblivion <export> ignition <export>
```

`<export>` is each map's umodel export, as `build_map.py` takes it (Ancients before
Oblivion: Oblivion's doors use Ancients' materials). Not in: the jets that fly past
Oblivion's observation deck once it is open (scenery, not a door).

### Escalation's traps

Three of the maps have the game's **traps** (`src\traps.rs`): floor consoles
(Escalation's `TnInteractCurrencyReceptacleMultiUse`) that set a trap off each time
they are bought, then come back to be bought again, run by the map's own script as the
door consoles are (`src\doors.rs`):

| Map | Trap | Consoles | Price | Back after |
|---|---|---|---|---|
| Downfall | **Defensive Sentries** | 3, one under each rocket sentry | 750 | 60 s |
| Oblivion | **Combustion Chamber** | 2, at the airlock (one total) | 250 | 60 s |
| Ignition | **Flame Jets** | 4, one at each jet | 250 | 30 s |
| Ignition | **Gunship Support** | 1, at the foot of the statue | 2,000 (500 a press) | 120 s |
| Ancients | **Electric Fences** | 1, by the lower fence | 250 | 30 s |

- **Buying one**: as with a door console ("Press E to deposit *250 of *750 to activate
  Defensive Sentries", the game's own line), it takes the price, folds away and goes
  dark. When its time is up it unfolds again, its outline back and its total at zero,
  whatever its trap's script did to it meanwhile (`ReenableAfterCooldown`).
- **Defensive Sentries**: Downfall's three rocket turrets hang from the hangar
  ceilings, folded up and asleep from the start of the match. A purchase wakes its
  sentry for 20 s. Awake, it takes the nearest enemy it can see (from its muzzle, as
  the game's `TnSentryPawn.GetViewLocation`) within 320 m, unfolds
  (lighting its ring and laser sights) and turns its head to it at 50 degrees a second.
  Once it is aimed it fires bursts of three homing rockets, half a second apart with
  2.5 s between bursts, each doing 75 damage to everything within 12.5 m. It folds up
  again when it has nothing to shoot at or goes back to sleep. As in the game's script,
  two of the consoles (one on the ground, the other on the walkway 20 m above it)
  switch each other off: buying one leaves the other off for the rest of the match.
- **Combustion Chamber**: Oblivion's airlock. A purchase sounds the alarm and switches
  on the chamber's alarm light, pulsing. After a second comes the blast's roar, and from
  1.25 s to 10 s the chamber burns (Exodus's flamethrower at 3.5 times its size and the
  incinerator's fire, `ESC_IncineratorFire_FX`), killing anything inside, players
  included (100,000 damage a second). The level's script never switches the alarm light
  off again, so it keeps pulsing after the first purchase.
- **Flame Jets**: Ignition's four jets. A purchase makes the jet's nozzle give its
  warning (its tell, with its sound), and from 1.3 s to 23.5 s it breathes fire down its
  corridor (Grimlock's flamethrower, half again as long: a white-hot tongue of fire
  from the nozzle into billows of flame 10 to 30 m out) with its roar, doing 20 damage
  a second to everything in it, players included, and 80 more to the enemies. Whatever
  it burns is set on fire for a second, and an enemy that dies burning blows up,
  hurting the players near it.
- **Gunship Support**: Ignition's gunship flies its 45 s circuit (its Matinee, with its
  engines): it comes down from high above, circles the arena 70 to 95 m up (passing
  right over the statue) and flies off. Its three guns (`MGInteract_TURRETDEF`) hang
  under it, unfold as it arrives and are awake for the flight. Each takes the nearest
  enemy it can see within 160 m and fires at it: 7 damage a round, faster as it keeps
  firing (from every 0.25 s down to every 0.08 s), with tracers and impacts. Its path
  is the Matinee's, relative to where the gunship waits, with its first key's turn
  taken out (that key is where it stands as the Matinee starts).
- **Electric Fences**: Ancients' two fences of lightning, one across the passage by
  the console (five strands, 2.5 m apart, between posts 29 m apart) and one across
  the gallery above (four strands, posts 25 m apart): nine emitters on the posts
  (`ElecFence_FX` at 1.25 times its size, each a crackling lightning beam and a
  wider one along it brightening what is behind, reaching 31 m toward the opposite
  post). A purchase switches them on for 15 s with their crackle
  (`EMIT_TRAP_ELECRTIC_GATES`, along the lower fence), each fence's pain volume
  doing 65 damage a second to everything in it, players included
  (`TnDamageTypeEnvironmentExplosion`: what it kills blows apart). Then they go off
  with a last zap (`EMIT_TRAP_ELECRTIC_GATES_OFF`), and, as the level's script has
  it, spark back on for a second at 16 s before dying at 17 s.

The traps' flames are lit by the level's lights around them, as the game lights them
(their emitters' particle components accept lights and have a light environment:
the `emitter` lines' last field in `levels\<map>_doors.txt`), unlike the effects the
game spawns. A trap's kills are nobody's, so each player gets half of what they pay (the game's
`SharedKill`). Online, every PC plays the traps itself as the purchases reach it, its
sentries and fire hurting its own copies of the enemies. The sentries' kills are in the kill
feed as the game shows them, "Sentry", the sentry icon and the enemy's name (see *The kill
feed*). Not in: the enemies attacking the sentries, and the one trap besides these,
Oblivion's **Security Drones** console (not drawn: Oblivion's model has no console
there).

The sentries' models (`models\traps\sentry_rocket.glb` and `sentry_mg.glb`, with
their clips, sockets and paint in `models\traps\traps.txt`) come from
`tools\build_traps.py`, from umodel's exports of their meshes and clips (its header
lists them), their texture set at its full size from CookedPC (the maps keep only
its small mips; the rest are in `WEP_SentryBlaster01_p.xxx`):

```
python build_traps.py <export> "<CookedPC>" ..\assets
```

Where the traps stand and what their consoles do (`levels\<map>_doors.txt`'s console,
emitter, light, pain and sentry lines) come with the doors from `tools\build_doors.py`.

### Escalation's downed players, revives and lives

With two or more players in a match, running out of health takes Jazz **down**
rather than killing him, as Escalation runs it (`src\revive.rs`, from
`TransGame.xxx`, the Escalation map packages' `TransContent` and the game's config).
Playing alone, he dies at once, as the game's `GlobalCanBeDowned` has it.

- **Down**: he falls (`Revive_Fall`), back in robot form if he was a vehicle, and lies
  where he fell. He can look round and **fire his gun** (a heavy weapon gives way to
  his other gun), his body aiming after the view (the game's nine downed aim poses)
  and turning over where he lies once the view has turned past 75 degrees. He can't
  move, jump, dash, melee, fine aim, transform, use his ability or support items,
  pick up or buy anything, or switch hands, and his shield doesn't recharge. The
  camera comes in close over his right shoulder with a 60-degree view (the downed
  camera, placed as the game's own video shows it).
- **The downed meter**: his downed health, 540 points, drains at 18 a second (on
  Normal: 30 s; Easy 43 s, Hard 20 s). For his first 3 s only the drain hurts him;
  after that every hit takes it down too. The HUD swaps its corners for the game's
  meter: "YOU ARE DOWNED", "HOLD G TO SELF DESTRUCT" and the red ring round the downed
  icon running down. Holding **G** for a second ends it at once; at 0 he dies.
- **Reviving**: a teammate on his feet in robot form within 5 m is offered "Hold E to
  Revive" for the nearest downed player nobody else is reviving. E held 0.3 s starts it:
  he kneels to it (`Revive_Heal`, 3.8 s, held where he is) under "Reviving *NAME*" and its
  bar, and the downed player's drain stops. Letting go stops it; held to the end, the
  downed player gets up (`Revive_Getup`) with 115 points of health, and
  the reviver's REVIVES on the scoreboard go up. The others see "AN ALLY IS DOWNED" and
  "FIND AND REVIVE YOUR ALLY" at the top, and the downed player's marker shows his downed
  health in red with the downed icon strobing beside it.
- **Dead**: there is no respawn timer. The dead watch their death for 3 s (dying down,
  the camera eases back out from the downed one to look on at his body, held still,
  with the game's death camera's 90-degree view), then **spectate** a teammate who is
  still up (downed counts as up): the camera cuts to him and glides in behind him over
  1.5 s, the mouse turning it round him and **[** and **]** going from one teammate to the
  next, under the game's respawn screen: "WAITING FOR NEXT WAVE TO RESPAWN", "CURRENTLY SPECTATING" and
  the teammate's name, with "[ Cycle Players ]" and the screen's soft dark edges (the
  game's own hint on a PC: the orange **[** and **]** are the keys to press; with a
  controller, the LB and RB bumpers' pictures stand at its ends, and the bumpers go
  through the teammates). With nobody to watch, the view is the level's camera. The announcer says "An ally was
  destroyed" for every death (Megatron on the Decepticon maps).
- **The team's lives**: the team has 3, shown as emblems at the bottom left (the
  Decepticons' on Downfall and Oblivion, the Autobots' on Ancients and Ignition). Once
  nobody has been up for 3 s, the team loses one (its emblem goes red) and the wave is
  lost: its enemies go, "Wave 1 Failed, Restarting Wave" shows with the wave's stinger,
  the dead come back at their starts, the downed get up, and the same wave comes again
  after the countdown (see *Escalation's waves*). A wave won brings the dead back and
  the downed up too. Losing the third life ends the game: the HUD's power-down plays
  and the **final scores** come up over the view ("YOU SURVIVED UNTIL WAVE 4", the wave
  the team reached, over the players' list, the game's `EndGameStats_GFX`), and after
  10 s every player is back in the lobby.

Online, each PC runs its own player's downed state, drain and revives and sends them
with his Jazz (the network protocol went to version 26 for it); the host keeps the team's
lives and sends them to the others with the lobby's roster. As in the game, the count
starts with the first player who is up in the match: nobody is while the match starts
(Choose Character and the countdown), and another player's Jazz only counts once his PC
has said he is in the match (it came in counted as up before his PC's first word, and
the three lives ran out during the countdown, ending the game before it began).

The downed clips (`models\robot_extras.glb`) come from `tools\build_robot_extras.py`;
the HUD's parts from `tools\build_hud.py`, which grafts the respawn screen
(`UI_GFxRespawn_p.MultiplayerRespawn_GFX`, from `mp_shared.xxx`) into the HUD; the
sounds from `tools\build_sounds.py`. Not in yet: the subtitle under the announcer's
line. (The kill feed's death notices at the top left: *The kill feed*.)

### Escalation's enemies

The enemies come from the game's own data (`tools\enemy_data.py` reads it, and
`tools\build_enemies.py` builds the models; both are done for you):

```
cd tools
python enemy_data.py "<CookedPC>" enemy_data.json
python build_enemies.py --umodel "<CookedPC>" <export folder>
    (prints the umodel commands that export the meshes, textures and clips it reads)
python build_enemies.py <export folder> "<CookedPC>" ..\assets enemy_data.json
```

Each map's spawn list (`TR_Survival30_m.SurvivalSpawnList_<Map>Tuned_p`) names the
types its waves bring:

| Map | Enemies |
|---|---|
| Downfall | Autobot soldiers, Scrappers, Rocket Troopers, Shotgunners, Snipers, Heavy Gunners, the jets (Flyers), Sharpshooters, and the Titan with his Shield and Explosive Drones |
| Oblivion | the same but for the Snipers and the jets |
| Ancients | Decepticon soldiers (the Rust Sea's), Rocket Troopers, Swarmers, Shotgunners, Snipers, Heavy Gunners, the jets and the Guardian |
| Ignition | Scrappers, Decepticon soldiers (Kaon's), Rocket Troopers, Shotgunners, Snipers, the Leaper, the jets and the Guardian |

(The table is the roster `enemies.txt` holds: `map` lines.) The waves bring them in,
wave by wave, as the game does (see *Escalation's waves*); the soldiers, Scrappers,
Shotgunners, Rocket Troopers, Heavy Gunners, Sharpshooters, the Guardian, the Leaper,
the Swarmers, the Titan with his drones, the Snipers and the jets have the game's AI
(see *The enemies' AI*). The test enemies that stood near the start of every map are
gone.

`assets\models\enemies\`:

```
<type>.glb        its skeleton and meshes (every part of every slot: one is shown,
                  picked as the game picks it), the gun it wears on its arm (the
                  Guardian's arm cannons, the Rocket Troopers' launchers, the Titan's
                  and the Heavy Gunners' X18s: each on its own bones), and its own
                  clips
npc_clips.glb     the soldiers' idle and deaths, on their shared skeleton
                  (TR_Skeletons_p.RB_NpcAi_SKEL)
textures\*.ktx2   its texture sets (colour with the paint mask, normals,
                  metal-roughness, glow), block-compressed
enemies.txt       the types: health, armour, size, energon colour, colour pairs,
                  slots, physics bodies and joints, sockets (their guns' too), clips,
                  the money a kill pays, and each map's roster (see
                  build_enemies.py's header)
deaths.txt        the death sets the enemies and the players die by: each
                  death's timeline and the destructibles they spawn
                  (tools\build_death_sets.py; see its header)
```

Only the map's types load, as the match starts (their models' meshes and pictures
are kept only where they are drawn).

**How they die** is the game's own data, played as the game plays it (`src\deaths.rs`,
reading `assets\models\enemies\deaths.txt`, which `tools\build_death_sets.py` writes:
`python build_death_sets.py "<CookedPC>" ..\assets`). Each type's chassis names its
death set (`TR_Deaths_p.Sets`): the soldiers' `Basic_DEATHSET`, the Titan's, the
Guardian's, the Leaper's, the Swarmers', the Snipers', the drones' and the jets'. The
game picks the first of a set's deaths whose death types the kill has
(`TnDeathPlayer.Pick`): the side the blow came from, whether it stood still, its form
(a robot, or a vehicle boosting or hovering), the damage type's `DamageDeathTypes`, a
headshot, being in the air, a gib (the killer within the damage type's `GibDistance`:
the Scatter Blaster's and the Energon Harvester's 15 m, a homing rocket's 7 m); a
conditional set picks one of its deaths at random by their weights. Then it plays that
death's timeline (`TnDeathTimeline`: keyframes of actions started together, each after
its own delay, the next once they are all done):

- a clip (the soldiers' 16 gunfire deaths, their headshot, their electrocution, the
  slime's stagger and the Energon Harvester's reach; the big ones' own);
- the ragdoll taking over (`TnDeathActionRagdoll`, with the velocities the clip left),
  thrown by the kill's momentum (the blow's, over the pawn's mass: `TearOffMomentum`)
  and its ragdoll actions in their order: a blast's radial impulse (from the pawn,
  where it was struck or a bone, as its `ExplosionOrigin` says), impulses on its
  body (on the bone each names, or the bone a round struck; at the point it gives
  them, where it was struck, turning the body as well as throwing it), its bodies
  halted, pieces severed (the head, an arm, a few joints at random near where it was
  struck) with their trails of sparks or smoke;
- particle effects on its sockets and bones (`TnDeathActionPlayEffect`: the sparks, the
  explosions, the slime, the Harvester's drain), lights (`TnDeathActionTriggerLight`),
  sounds (the `BL_DEATH` cues; its death cry is its voice's), camera shakes, its
  gravity changed, spins, a plane's boost kept on;
- destructibles (`TnDeathActionSpawnDestructible`, `src\gibs.rs`): gibs (the game's
  chunks, the Swarmers' own pieces and egg pods) that fly out, bounce off the level and
  dissolve with their sounds, particle effects, energon decals, and a
  blast's damage on the players near it (the Titan's going up, a thrown body's);
- and the dissolve (`TnDeathActionHighlight`'s `TnDeresser`, and its sound): the
  game's overlay (`TR_DefragOverlay_MATINST` for nearly every death) drawn over the
  body as its `DefragValue` goes from 0 to 1, as the Cloak's is: a ragged front out
  from the body's middle to its bounds' radius times 1.2 (times the death's
  `RadiusAdditionalScale`), the body black behind it with a cyan rim, the lines of
  the game's `IaconMask_NORM` glowing blue across it, then burning out in blue; the
  body itself is drawn no more once the value passes
  `NormalizedTimeToMakeMeshInvisible` (0.397), only the overlay. Every enemy and
  player dissolves in the game's cyan, whatever his faction; the electrocutions'
  overlay is pale blue (its noise the game's `ElectricArcs`), the corrosion's brown
  and the fires' red, as the game's are (`src\enemies.rs`'s `DEFRAG_LOOKS`,
  `assets\shaders\paint.wgsl`'s `defrag_overlay`; `tools\build_cloak.py` writes its
  textures and checks its values).

The Titan goes up 5.5 s after he dies, the Guardian soon after he falls; a drone shot
down throws off sparks, one that blew itself up bursts. Online, the PC whose player
landed the killing blow tells the others how it died, with the blow's momentum and
whether it gibs, and every PC makes its death's choices alike, so they all see the
same death.

**The ragdolls** (`src\ragdoll.rs`) are the physics assets' bodies (spheres,
capsules, boxes and the jets' convex hulls), with the masses the assets give them
(the rest by their size, as PhysX weighs them), held together at their joints, within
their swing and twist limits, falling under the game's gravity onto the level's
collision (or the arena's floor), with friction. The game's physics engine (PhysX)
isn't in the demo, so this is a small solver of its own: a joint that a pose puts
past its limits is eased back rather than snapped, a piece locked to its parent (the
Guardian's backpack plates) moves as part of it until a death severs it, a body that
starts in the floor rises out of it rather than being thrown, nothing bounces off the
floor, and a body that has stopped moving (or stays where it is, only rocking) comes
to rest within about half a second. A test drops every enemy's ragdoll and the
players' and kicks it as the deaths do (gunfire, a shotgun's throw, a blast, a cut in
two, a melee blow) to see that each lands whole and comes to rest.

**The players** die by the soldiers' death set too (every class's robot names it)
and fall as the shared physics asset, `TR_Shared_PHYSICS_p.RB_Shared_PHYS`.
`tools\build_deaths.py` writes their clips (16 gunfire deaths, the headshot, the
electrocution, the stagger and the reach, from `Death_Shared_ANIM`, on Jazz's
skeleton) and the physics asset:

```
python build_deaths.py <export folder> "<CookedPC>" ..\assets
assets\models\robot_deaths.glb    the death clips
assets\models\robot_deaths.txt    their roles and the physics asset's bodies and joints
```

Without them the players fall in the old death pose. Left out of the deaths for
now: the dying enemies' hit reactions, and the few effects the game itself never
draws (see *The Riot Cannon*'s note on the game's shaders).

### Escalation's waves

The enemies come in **waves**, run as the game runs them (`src\waves.rs`): each map's
survival actor and its goals (`TnSurvivalGoalTimePressure`,
`TnSurvivalGoalWaveSpawning`), its spawn list
(`TR_Survival30_m.SurvivalSpawnList_<Map>Tuned_p`) and the game's spawn point manager
(`TnSpawnPointManager`, in `TransGame.xxx`).

- **The countdown**: 10 s after the match starts, the countdown to the first wave
  (30 s) shows at the top right of the HUD, the game's own clock
  (`mc_scoreboardSingleTeam`), beeping each second from 10 and flashing red. With 7 s
  left the announcer (Optimus Prime; Megatron on Downfall and Oblivion) says the next
  wave begins in..., then counts "Five" to "One", each line typed under the
  announcements ("Wave 1 Begins In...", "Five..." to "One..."), with the wave's stinger on
  "One".
- **A wave** brings one of its spawn groups, which names its enemy types, what each
  costs, how many of it may be in at once and how often it comes (its weight). They
  come in one at a time, 1 to 3 s apart, while the wave's living budget has room (12
  enemies at most on Downfall, 14 on the other maps), until its total budget is spent. "WAVE 1 OF 15" types itself in
  under the energon counter as the wave starts (`mc_currentWaveMessage`) and stays.
- **Where they come in**: each type has a spawn request, which the map's spawn
  clusters serve. The spawn point manager scores those clusters ten times a second
  for the players who are up: out of their sight (all but Ancients' Swarmers, which
  don't mind being seen) and near them scores best. It keeps one open for up to
  10 s, unless another beats it clearly. The clusters behind the locked doors stay
  shut until their doors open.
- **Stranded enemies** (the demo's own): an enemy that no player who is up has seen
  for a minute (from his eyes or his middle to its middle or its head, the level
  and its closed doors in the way), or for 30 s once the wave's last few show, is
  taken out of play (not killed: no reward, no death) and its cost given back, so
  another comes in where the spawn point manager now puts it, near the players.
  One stuck where nobody can get at it (behind a locked door's force field, up a
  scaffold) can't leave a wave unwinnable.
- **The last few**: once all of a wave's enemies are in and 3 or fewer are left,
  their markers show through walls on every player's HUD, with the scan's sound, and
  the announcer says "We are 3 kills from victory!" and then "We are 1 kill from
  victory!" ("Three Enemies Remaining", "One Enemy Remaining").
- **A wave won**: "Wave complete" ("Wave 1 Complete", with its stinger). Every player
  gets the wave's reward (200 energon for waves 1 to 5, 400 for 6 to 10 and 600 after),
  the dead come back, the downed get up, and the 30 s countdown to the next wave
  starts.
- **A wave lost**: when nobody has been up for 3 s, the team loses a life (see
  *Escalation's downed players, revives and lives*). The wave's enemies go, "Wave 1
  Failed, Restarting Wave" shows with its stinger, every player gets 500 energon and
  the same wave comes again after the countdown.
- **Harder as it goes**: through each wave the players do less damage (down to half
  from wave 11) and take more (up to 1.8 times from wave 9), on top of the
  difficulty's: the damage they take (Easy 0.7, Normal 1, Hard 1.5 times) and the
  damage they do (Easy 1.5, Normal 1, Hard 0.6 times). The enemies themselves are
  the same on every difficulty, as in the game: their guns, their aim and their
  numbers (the waves are the same for one player as for four).
- **The end**: losing the last life, or winning the 15th wave (5 s after it), brings
  up the final scores: "YOU SURVIVED UNTIL WAVE *n*".

Each wave's enemies are the types its spawn groups name:

| Map | Wave 1 | Then |
|---|---|---|
| Downfall | Autobot soldiers, Scrappers | Rocket Troopers from wave 2, Shotgunners 3, Snipers 4, Heavy Gunners 5, jets 6, the Titan 10 (and 14, 15), Sharpshooters 12 |
| Oblivion | Autobot soldiers, Scrappers | Rocket Troopers from wave 2, Shotgunners 3, Sharpshooters 4, Heavy Gunners 5, the Titan 10 (and 14, 15) |
| Ancients | Decepticon soldiers (the Rust Sea's) | Rocket Troopers from wave 2, Swarmers and Shotgunners 3, Snipers 4, Heavy Gunners 5, jets 6, the Guardian 7 |
| Ignition | Scrappers, Decepticon soldiers (Kaon's) | Rocket Troopers from wave 2, Shotgunners 3, Snipers 4, the Leaper 5, jets 6, the Guardian 9 |

(Not every wave after brings all of them: wave 3 on Ignition is Shotgunners, soldiers
and Scrappers, for one. `levels\<map>_waves.txt` lists each wave's spawn group.)

Every type fights as the game's does (see *The enemies' AI*). The jets and the
Snipers come in at the level's air spawn points (`TnSurvivalAirSpawnPoint`) and fly
on from there. Ignition's
Leapers come in far outside its walls, where the game's spawn animation leaps them
in, so they glide into the arena (the animation itself isn't played yet), and fight
from there. The other spawn animations aren't played.

Online, the host runs the waves and sends them with the lobby's roster (the
countdown, the messages, the markers, the rewards and the enemies in play, which
every PC brings in as the host does), and an enemy killed on any PC dies on the
others too, the same death (the network protocol is now version 34).

The waves' data comes from `tools\build_waves.py`, which reads the maps' packages
(`ESC_<Map>_BASE_m.xxx`, `TR_Survival30_m.xxx`) and writes `levels\<map>_waves.txt`:

```
cd Documents\foc_esc\tools
python build_waves.py "C:\...\Transformers Fall of Cybertron\TransGame\CookedPC" ..\assets
```

### The enemies' AI

Escalation's soldiers (the Autobot soldiers of Downfall and Oblivion, the Rust Sea's
Decepticon soldiers on Ancients and Kaon's on Ignition), Scrappers, Shotgunners,
Rocket Troopers, Heavy Gunners, Sharpshooters, the Guardian, the Leaper, the
Swarmers, the Titan with his drones, the Snipers and the jets fight as the game's do: `src\ai\` is Fall of Cybertron's HTN AI (`TnHtnAiController` and its planner,
sensors, operators, motion, weapons and animation) ported from the game's script,
each type set up as its blueprint class sets it up (`src\ai\classes.rs`: its sight,
health, motion, gun, stance and sensors) and run on its own planning domain,
behaviour values, animations and sounds from the game.

- **Thinking.** Each enemy keeps the game's world state (50 attributes: whether it
  sees an enemy and how far he is, its cover, its alert level, the squad's actions
  open to it...) from its sensors, and plans for its goal with the game's planner on
  its type's planning domain (the soldiers' is `TnSoldierCharacterBlueprint`'s 108
  tasks, from the game): a new plan whenever the world changes, the better one (by
  the order of the methods it took) taking over when the task running can be
  interrupted, its operators run one after another (move, take cover, peek, shoot,
  reload, throw a grenade, strafe, melee, react...). Escalation's alerter sends every
  enemy at the first player every 5 s, as the game does
  (`TnSurvivalGoalModifyAIBehavior`).
- **Senses.** Sight (in its view cone, after the game's perception delays), hearing
  (the players' gunfire and the shots that nearly hit it, its buddies' deaths), pain
  (who hurt it), threats (which player is its enemy), its squad's broadcasts, and the
  tactical system's choice of where to fight from (the map's tactical nodes, scored
  by the blueprint's evaluators for cover and line of sight to its enemy).
- **Moving.** On the map's navigation mesh (from the game's own: `levels\<map>_nav.txt`):
  paths found across its cells and pulled tight round the corners (none through a
  locked door until it opens, as the maps' Kismet blocks them), out through the walls
  round the spawn areas that keep the players out (the game's player-only blocking
  volumes, which let the enemies through), walked, jogged or run
  with the game's locomotion and obstacle avoidance (steering round the soldier it
  would soonest run into), jumps where the mesh has them, cover taken and left,
  strafing while it shoots.
- **Where a player is.** The enemies go for the place on the mesh where each player
  is (`TnPawn.UpdateNavigableNearbyLocation` and `FindNavMeshLocation`): his feet or,
  while he is in the air (jumping, falling, a jet), the ground straight under him
  within 25 m (the trace down his `NavigableLocationCheckDistance` makes), over an
  open cell of his own floor, one the enemies walk too. Off the mesh (on a crate or
  a ledge, a jet high up) they keep the last place he was on it, as the game does.
  Any cell up to 2.5 m under his feet used to count. Oblivion's ramp out of the
  enemies' sunken spawn area runs under a players-only lid (an invisible floor that
  holds the players up and lets the enemies through), so a player standing on it
  sent them to the ramp's cells right under him. There they stood under the floor,
  out of his reach. Now they wait where he left their floor.
- **Physics.** Each one walks, falls and flies as the game's pawns do
  (`TnPawn.CalcVelocity`: its acceleration and braking along each of its own axes,
  the momentum it keeps, its ground, air and terminal speeds), standing a hair over
  the floor as UE3's walking pawns do (2.15 units: Ignition's leap smash volume is
  built to it) and on any floor under its cylinder (a big one rests on a ledge's lip
  rather than dropping off it); a fast one (a Leaper's dive) meets a wall as it comes
  to it, a jump stops under a ceiling, and coming down it lands as the game's
  acrobatics do (`TnAcrobaticsManager`: its landing animation for how fast and how
  far it fell). A jump down to a landing comes down where its middle is over a
  floor. Landing anywhere under its cylinder, Scrappers jumping off Oblivion's upper
  deck (the 10 m drop by path node 1194) came down on the deck's lip, were held there
  and stayed, out of reach (8 of 20 brought in up there in a test). A jump that
  comes down short of its landing now ends the move and the enemy plans again: the
  game's melee move (`TnMoveToOperator`) stays in its jump then, and froze them.
- **Hit reactions.** Every blow makes it flinch (the additive hit reactions, by where
  it was struck), and the more damage it has taken in the last 3 s the likelier a
  blow interrupts what it is doing (`TnHitReactionSensor`, out of 150): in cover it
  reacts as its cover's pickers say, and struck by a round while it moves it
  stumbles (the game's shared `TR_AI_Navigation_HitReactionPicker`: backward, to
  either side, or forward at a sprint, by the way it moves, how fast and the bone
  the round struck; none of them fitting, it doesn't stumble and goes on at once),
  its root motion carrying it several metres. A melee blow names
  no bone (the sword's sweep meets its collision, not its body's bones), so it
  doesn't stumble from one; the blow moves it instead (`Pawn.AddVelocity`: the
  blow's momentum over its mass, 7.5 m/s away from the striker and 7.5 m/s up for a
  soldier struck by the sword, which takes it off the ground). A blow that would
  keep it in the air for over half a second (`TnHtnAiPawn.TakeDamage`'s
  `EstimateAirTime`: the sword's on a soldier, 0.51 s) **knocks it back**: the
  melee bit of its `WS_IsInPain` has its planner take `STS_Knockedback`, whose
  operator (`TnKnockedbackOperator`) throws it with the blow's velocity
  (`DoKnockBack`), plays its blueprint's knockback take-off, holds its flight
  clip while it is in the air and plays its landing as it comes down. `TnPawn`'s
  `bIgnoreForces` keeps rounds and blasts from moving the enemies; the melee damage
  type asks for its forces (`RequestRespectForcesApplied`), and none moves one an
  animation's root motion is moving.
- **Fighting.** Its rifle in bursts (how long, how often and how well aimed by the
  competency the game works out for it), its spread, clip and reloads, the damage
  falling off with range. Aimed at a player's view point (his robot's head bone,
  `C_Spine04_Head_XB`: `TnPawn.GetViewLocation`), its rounds strike what the game's
  traces strike of him: his robot's body, its physics asset's shapes where his bones
  are (in his vehicle, its box). His collision cylinder, much wider than his body
  (the Infiltrator's 2.8 m across), lets them through
  (`TnPawnCollisionController`: `SetTraceBlocking(false, true)`), so a pellet or a
  badly aimed round passing beside him misses him. Melee for a player who comes
  close (`Soldier_MS`; each swing's animation calls back for that swing alone, so
  one given up part way never ends the swing begun after it); and
  grenades: a squad's soldier may claim the throw at most every 30 s
  (`GrenadeThrowInterval`), at a point near its enemy the squad's claim finds, thrown
  as its throw animation lets go.
- **Shotgunners.** Their shotgun fires its eight 6-damage pellets in the game's
  spread pattern (`Shotgun_AI_WEPDATA`), a round each 1.4 s or so (each burst one
  round, then its 1 s `BurstDelay`), each pellet striking his body or missing it as
  above: up close all eight land, further out and aimed badly fewer. A
  player who comes close sends them dashing back, firing as they go
  (`TnAnimatedShootingAttackOperator`), stepping aside round him as far as the
  ground lets them (`TnSideStepOperator`), or backing away from him
  (`TnMoveAwayFromEnemyOperator`).
- **Rocket Troopers.** They carry a rifle and a rocket launcher, taking up and
  putting away each as their plan wants it (`TnEquipWeaponOperator`,
  `TnUnequipWeaponOperator`); the launcher's rockets fly as the game's do, with
  their trail, and burst where they strike.
- **Sharpshooters.** They cloak whenever they like (their cloak never runs out), and
  before a shot they lay their laser sight on their enemy, holding still while it
  closes in on him until it has locked on (`TnScanSniperEnemyOperator`): its three
  beams spread round him and closing in as it locks on (see *The Snipers*).
- **Scrappers.** They fight hand to hand as a squad: one at a time goes at a player
  while the rest wait their turn round him (`TnIdleInAttackChannelOperator`),
  stepping out of each other's way (`TnMoveToClearAttackChannelOperator`), and react
  to the blows they take; one whose blow a raised Flak Shield stops shows its
  frustration. **One thing is the demo's own:** the game counts a Scrapper's way
  to a player as clear only along a straight walk across the navigation mesh
  (`TnClearAttackChannelSensor`), so one below a player on a ledge side steps for a
  clear way for ever, however near the way up is (on Downfall, the trench under
  the Photon Burst Rifle's walkway). After two side steps in a row that leave the
  straight way cut off, a path to him on the mesh, jumps and all, counts as clear
  (`src\ai\sensors.rs`'s `WayRound`): its move to him (`SCRAPPER_MoveTo`, which
  follows paths) takes it up, and the game's own rules take over once the straight
  way is open. The test `downfalls_scrappers_climb_out_of_the_trench` checks it.
- **Heavy Gunners.** Their machine gun's fire is dragged across the place they track
  (their enemy, while they see him: `TnShootAtTrackedLocationOperator`) as they walk
  or stand, until it overheats and cools (its reload); badly hurt, they stumble. The
  gun is the Titan's, an X18 on the arm (their `TnWeaponHeavyGunnerMG` sets its own
  rounds, 5 damage and 150 to a clip, and takes the rest from the Titan's
  `TnWeaponTitanHeavyMG`, whose class it is: its `WeaponMeshTemplate`,
  `TitanHeavyMG_WEPMESH`, among them). It used to be missing, the build reading only
  the class's own defaults, so their rounds left from in front of their chests; now
  they wear it and fire from its barrels. Both the Heavy Gunners' and the Titan's
  fire from its two barrels by turns (its `MuzzleSockets`, `MuzzleFlash` and
  `MuzzleFlash2`, with `TnWeaponEffectIncrementMuzzleSocket`): each round's flash,
  tracer and trace from the barrel it is on, and with every round a flash on the
  second barrel besides (a `TnWeaponEffectPlayFXOnSocket`, held on it and started over
  each round: `PEC_RestartIfAlreadyPlaying`). Every round shows its tracer
  (`WEP_TitanMG_Tracer_FX`: a spray of sparks streaking from the barrel to where it
  struck), and its muzzle flash has its brightening glow and its clouds (see *The
  Riot Cannon*'s note on the game's shaders).
- **The Guardian.** It has the game's two forms (`TnGuardForms`). In its investigate
  form it moves slowly, its arm cannon put away, sweeping its scanner beam (a fan of
  green light 50 m long and 60 degrees high out of its head, in the game's own
  material, cut short where the level stops it) across its patrol points and the
  places it suspects. A player the beam falls on, in its sight, is exposed (his
  cloak torn off, with the game's decloak sound), seen at once and jammed: for 10 s
  his ability can't be used or refill, and the HUD shows it amber, JAMMED. It takes
  half the damage of a blow in that form, but twice that to its scanner head.
  Alerted, it changes to its battle form (fast, its Guardian Blaster's cannons on
  both arms, taking 35% of a blow): the blaster charges for 5 s, glowing and humming, then
  fires its burst of homing rounds (30 damage each, with splash); and when its enemy
  is out of its sight or out of its reach it lets off its blast: 50 damage to every
  player within 20 m, thrown back and up, and jammed.
- **The Leaper.** Ignition's Leaper leaps (`TnLeapSmashOperator`): standing inside
  the map's leap smash volume (`TnLeaperSmashVolume`, `TnLeaperSmashVolumeSensor`)
  with its enemy in sight 35 to 70 m off (nearer only when it can't walk straight to
  him), the place it would land, 5 m short of him, inside the volume too and its
  leap's arc clear of the level, it claims the leap (its squad's, one Leaper at a
  time, each one's 3 to 5 s apart), gives its tell facing him, and jumps 25 m up;
  over the top it turns into its dive and comes down at 100 m/s on where he is by
  then (within 15 m of where it aimed), smashing every player within 15 m (100
  damage, thrown back), and recovers on the spot. In the air it takes 7.5 times the
  damage. Nearer, it lunges at him on its boosters (`TnMeleeLungeOperator`: 50
  damage to what its sweep meets), skids to a stop (`TnAnimatedSkidOperator`, its
  sweep hurting by how fast it still goes), swings its fists (40), and when he keeps
  within 12 m of it for 2.8 s (`TnEnemyKitingSensor`) blasts him away with its
  booster vents (a shockwave from its animation's impact sweep,
  `TnAnimNotify_ImpactDamageSweep`). Its back is its weak spot: a blow from behind
  does three times the damage, a melee blow from behind fifteen times that.
- **Swarmers.** Ancients' Swarmers (`TnSwarmerCharacterBlueprint`, in their insect
  form, `TnInsectFormBlueprint`: Escalation's SwarmerLite) fight as the Scrappers
  do, as a squad round a player (their kung fu circle, 20 m, each one's turn in its
  attack channel, with no wait between turns), scuttling at 26 m/s and biting with
  their insect form's melee set: its blows' 150 damage at 0.14 (the Escalation
  Swarmer's `TnBuffModifySwarmerMeleeDamage`), 21 a bite. A player in his jet is out
  of their reach: they mill about; a melee blow knocks them back. They chirp and
  rattle as their animations do (the cues their dialog notifies name:
  `HmPawn.PlayDialog`).
- **The Titan and his drones.** The Titan (`TnTitanCharacterBlueprint`) fights on
  its own domain (its cannon, its swings, closing in as it shoots) and sends out its
  drones as the game's does (`TnDroneSensor`, `TnSpawnDronesOperator`, the drones'
  own planning domains, which their characters build in script): as its health
  falls under 67%, and again under 27%, it calls for them and flings two **shield
  drones** out of its back. Each flies to its post 20 m out round it, 2.56 m over the
  floor and turned away from the others (`TnHoverDefendActorSensor`), holds it, and
  beams to its chest (`ChainBeam_MainBeam_FX`): while any of them lives nothing
  hurts the Titan (`TnBuffSupportDroneDamage`), its shield's arcs playing over it
  (`ShieldHeal_FX`), and once the last is shot down (75 health each) the shield goes
  and it says so. When it can't get at its enemy (up out of its reach) it sends
  three **explosive drones** at him, at once and every 20 s after: each flies at him
  glowing (`DroneFlight_FX`), stops 5 m off, lights its fuse (2 s, pulsing) and blows
  up: 50 damage to every player within 10 m it can see. Out of his sight it goes
  where he was last seen, then back to its Titan; it lives 60 s at most. The drones
  fly as the game's hover planes do (`TnAiHoverPlaneMotionController` driving
  `TnHoverPlaneSimulation`: their rigid body steered at where they are going, leaning
  into their turns and moves, along their own way over the navigation mesh,
  `TnDronePathFollowingSensor`), humming with their engine, and die with their Titan.
  The game's overshield look on the shielded Titan (its character material's
  overlay and outline) isn't drawn.
- **The Snipers.** Downfall's, Ancients' and Ignition's Snipers
  (`TnSniperCharacterBlueprint`) have the game's two forms, each its own model and
  anim tree: a flying robot and an insect that clings to walls and ceilings. A change
  of form plays as the game's transformations do (`TnTransformation`: the form it
  leaves plays its half on its own model and the form it takes its half on its own,
  each shown and hidden as their clips' `TnAnimNotify_ToggleHidden` say, the pawn's
  collision taking the new form's). They come in at the level's air spawn points and
  fly, boosters lit (`TnAiSniperBoosterEffects`), along the level's air network to its
  sniper points (`TnSniperPoint`: patches of wall, each claimed by one Sniper at a
  time), take their insect form onto the wall over half a second
  (`TnAttachToWallOperator`) and fight from there: they crawl about their patch
  (`TnCrawlOnSniperPointOperator`), turn to aim at a player they see (their insect's
  aim offset node, `HmAnimNodeAimOffset`, by where they aim off their wall:
  `TnSniperAimAnimator`), lay their laser sight on him and fire when it has locked on
  and held (75 damage a round, out to 200 m). After each shot they look for another
  patch near them that sees him (`TnChooseNewSniperAttackPointOperator`) and fly
  over to it if there is one (`TnDetachFromWallOperator`: back into the robot, off the
  wall and turned to their next point); when they lose him they search from the
  patches round theirs. They steer their flights themselves (their acceleration, top
  speed and turn rate their own: the game gives them no motion controller, so the
  orders a walker's controller would take, such as stopping when a task takes
  motion away, do nothing to them), and a patch counts as taken while the map's
  Kismet blocks it as well as while a Sniper has claimed it (`TnSniperPoint`'s
  `get_IsClaimed` is its `bBlocked`): on Downfall that is every patch beyond a locked
  door, until the door is bought. A flight of the air network that passes through a
  locked door, or a scaffold still standing, is shut until it opens (`src\nav.rs`'s
  `shut_door_links`), and each flight's room is measured against the map's own
  surfaces as the match starts (`measure_flights`, a few milliseconds a frame): one
  that grazes the map is narrowed to the room it has, and a Sniper (or a jet) too big
  for it goes another way; a Sniper's hop straight to a patch beside its own
  (`GetSurroundingPoints`) is one of these flights too, and it passes over a
  neighbour it hasn't the room to fly to. Coming in with no patch found along the
  network, it takes the nearest free patch in sight it can fly on from and has room
  to fly to straight (Downfall's `TnSniperPoint_9893` has no flights at all). In flight they are stopped only by the level's
  solid surfaces, not its blocking volumes: Ignition's AirStrike station's barrier
  (`DynamicBlockingVolume_16006`, the only one across any map's flight paths) holds
  the hub of nine of its flights (`AirPathNode_5790`), which the map's Kismet leaves
  open, and taken as a wall it held the Snipers headed through there against it. Hurt past half their health on a wall, they reel
  (their insect's `HitReactions`); no blow moves them. **The laser sights** (theirs
  and the Sharpshooters', `TnAiLaserSight`): the game's `Laser_Sight_FX` on its socket
  (the insect's head, the Sharpshooter's gun), three beams each a degree off where
  it points at first, closing in as it locks on and rolling round it at 200 degrees a
  second, each reaching to the gun's range or to what it strikes (the level, a
  player, another enemy: one beam traced every 1/15 s in turn), in its wielder's
  energon colour, and its scan's loop, heard from where the beam passes nearest you.
  Its light (which lights only the players, in the game) isn't drawn. Their shots'
  tracer (`Tracer_Sniper_AI_FX`) is drawn as the game draws it: its lightning bolts
  are sprites laid along their motion, and they take their motion from the tracer's
  own, which stands still, so they have none; the game's sprite shader leaves such a
  sprite no length and it isn't seen (it was stood upright here, a streak 25 to 30 m
  straight up from each sniper as it fired, like a lightning strike).
- **The jets.** The Flyers (`TnJetCharacterBlueprint`, every map's but Oblivion's)
  fight as planes and come down as robots (`src\ai\jet.rs`). As planes they hover
  (`TnHoverPlaneSimulation`, at 20 m/s) or fly (`TnPlaneSimulation`, at 40 m/s, 60
  boosting), steering at a point ahead of the nose and banking into turns
  (`YawDueToYawValue`, `RollDueToYawValue`), straight at where they go if the level
  leaves the way open, else along its air network (`TnPlanePathFollowingSensor`),
  and keep their distance from each other. Against a player on foot (or in a car,
  a truck or a tank) a jet flies to a point ahead of him and over him
  (`TnAirAttackPointSensor`: 30 to 80 m off and 15 m clear of the floor), hovers
  there turned to face him (`TnHoverToFaceOperator`; taken on to the point itself
  if it begins the hover within its controller's reach of it, and facing him
  whichever way it is turned right over him), taunts him and fires its
  **sticky rockets** for 3 s (`TnWeaponFlyerRocket`: one every 3 s from its two gun
  sockets by turns); hurt there it slips aside (`TnPlaneDashOperator`), and it
  chases a player who gets further than 100 m (`TnAerialPursuitSensor`). A rocket
  (`TnProjectileFlyerRocket`) sticks in what it strikes, the level or a player
  (riding him), its fuse lit: it goes off 3 s later for 60 damage within 10 m (all
  of it within 3 m, then half falling to none), shaking the views of those it misses,
  unless the player it is in **changes form**, which shakes it off (its detach flash
  and sound, the flash let go after a second, and it is gone); meanwhile his HUD
  says "<key> to Disable Sticky Rocket"
  (`TnContextualCommandFlyerRocket`), and its grenade marker shows while it is out.
  Every 8 to 12 s (one jet at a time, 10 s apart: the squad's `SAT_JetRobotAttack`)
  a jet flies to a point near its enemy and over him
  (`TnChooseJetRobotAttackPointOperator`), changes into its robot in mid-air, falls
  and lands (`Nav_DescentToLand`), blasts all round it (`Ability_PulseAttack`: 20
  damage out to 50 m, with its tell), jumps and changes back into its plane.
  **Against a player in his jet** it dogfights instead: it boosts to a point round
  him (`TnBestFlyingCirclingPointSensor`, 40 to 80 m off), takes its dogfight gun
  (`TnWeaponRocketVehicle`, the trucks' rocket launchers hung on its wings, out from
  its first shot) and fires a rocket every 0.75 s (18 damage within 5 m), head on
  while he and it face each other (`TnShootAtEnemyHeadOnOperator`) or round him
  while he flanks it (`TnShootAtEnemyFireForEffectOperator`). The planes play their
  tree (`VEH_Plane_ANIMTREE`: the hover poses by the way they move, the boost and its
  turns, the switches between them) and burn their hover jets (as hard as each
  pushes), afterburners and contrail in their energon colour, with their engine's
  hover and boost loops, its boost's start and end, and the flyby as one passes you
  flying (`VEH_JET_FLYER_01`).
- **Grenades.** The soldiers' grenade (`TnProjectileGrenade`, its Escalation data)
  falls and bounces off the level and off the players and the other enemies (not the
  one who threw it), its fuse lit as it first strikes something; 4 s later it goes off:
  50 damage within 10 m (all of it within 5 m) to each player it can see, and a camera
  shake. While it is out, a player within 18 m of it sees it on his HUD (the game's
  `mc_markerGrenade`): its icon blinking over it, or, when it is off the screen or
  behind him, held at the screen's edge (inside the safe frame) with a red arrow
  pointing to it.
- **Players' grenades.** A player's grenade that bounces to under 1.5 m/s
  (`NotifyPawnsOfGrenade`: an EMP grenade, a Chaos-Rift Combustor's cluster) warns
  every soldier within 20 m of it, who dives away from it (`STS_ReactToGrenade`, the
  reaction for the side it came down on).
- **Animation.** Each type's anim tree (the soldiers' `Soldier_ANIMTREE` and the
  others' own) worked out as the game's:
  its stance blends and their transitions, the velocity blends (moving in every
  direction at each speed; the Swarmers' strafer, by the way they go), turning in
  place (its turn's animation as it turns standing: the Swarmers', the Titan's and
  the Heavy Gunners'), the jumps, the aimer (the aim poses turning its gun to its
  target), the aim offset nodes (the Guardian's, the Leaper's and the Snipers'
  insects': each of their bones turned and moved by the offsets the game baked from
  their aim poses, by where they aim: `AnimNodeAimOffset`), the custom slots its operators play (reloads, throws,
  cover, melee, reactions), the additive hit flinches, and the whole pose mirrored while
  it holds its gun in its left hand. Root motion moves it as the clips do.
- **Effects.** Their guns' muzzle flashes, tracers, rounds and impacts, and the
  effects their animations play (the dust of a landing, sparks from a sliding foot,
  booster flames, swing trails...) at the sockets the game puts them on, carried
  with them as they move: the game's own particle systems (`tools\build_weapon_fx.py`
  writes them with the weapons').
- **Sounds.** Its footsteps (its clips' footstep notifies, the floor's sounds), its
  animations' sounds, and its dialog: each soldier is one of his type's two voices
  (the Autobots' `SLDA01` and `SLDA02`, the Decepticons' `SLDD01` and `SLDD02`), and
  every line goes through the game's dialog manager (`HmDialogEventManager`: each
  dialog group's limit, chance, retrigger delays and priorities), so the squad doesn't
  talk over itself: spotting an enemy, peeking from cover, throwing a grenade, coming
  in, dying (none for a headshot), a teammate coming in or dying near him...

**Online, the host's AI plays everyone's enemies**, as the game's server runs its AI
and sends its pawns to the other players (`src\ai\sync.rs`). Twenty times a second
the host sends each enemy as its AI has it (where its pawn is, its velocity and
turn, its physics, the pose its anim tree made, its gun in hand, its change of
form, its plane's turn, its laser sight) with what they did (each round's trace,
their rockets and grenades, their blows to the players and to the barriers, the
players their scanners caught, their sounds and dialog, the effects they played
on their sockets). On the other PCs the enemies think nothing: each is shown a
tenth of a second behind the host's clock, between two of its states (its place,
turn, pose and change of form eased between them), its models posed from the
host's pose and its gun's, scanner's and engines' loops held as the host's state
says, and what they did plays out there at the same delay: the tracers, impacts,
sounds, dialog and effects, the rockets and grenades (each PC flies its own copies,
whose blasts hurt its own player), and the blows the host's traces landed, which
each player's own PC takes. The Titans' drones come with the waves' enemies (in
the host's roster, numbered with them).

What the players do to the enemies goes as before: each PC plays out every
player's shots (a round names the enemy it struck by its number, so it strikes the
same one everywhere), rounds and blows on its own copies, and the host's AI takes
up the blows it took there (its hit reactions, its taking cover, its turning on
whoever hurt it); an enemy dies on the PC where it first runs out of health, and
its death goes to the others, played the same way everywhere.

The AI's data comes from the game too (the scripts' class defaults in
`TransGame.xxx`, the blueprints, anim sets and sounds in the shared and map
packages):

```
cd Documents\foc_esc\tools
python build_ai.py "<CookedPC>" ..\assets
python build_nav.py "<CookedPC>" ..\assets
python build_ai_anims.py <export folder> "<CookedPC>" ..\assets
python build_death_sets.py "<CookedPC>" ..\assets
python build_sounds.py "<CookedPC>" ..\assets
```

(`build_sounds.py` comes last: it reads the sounds the others' files name, the
deaths' too.)

```
assets\models\enemies\ai.txt          each type's planning domain and behaviour values
assets\models\enemies\ai_sounds.txt   each type's sound sets and voices, the dialog rules
assets\models\enemies\ai_anims_<set>.txt
                                      each anim set's clips, notifies, root motion and
                                      what its anim tree needs (soldier, shotgunner,
                                      rocket, scrapper, titan, guard, leaper, swarmer,
                                      drone, sniper and sniper_alt: the Snipers' robot
                                      and insect)
assets\models\enemies\ai_<set>.glb    its clips on its skeleton (the soldiers' 197;
                                      ai_<set>_mirror.glb: mirrored, for the trees
                                      that mirror)
assets\levels\<map>_nav.txt           the map's navigation mesh, jumps and tactical nodes
                                      (and Ignition's leap smash volume), its air
                                      network with its sniper points and air spawn
                                      points
```

`build_ai_anims.py` reads umodel's export of the enemies' anim sets (the export
folder `build_enemies.py` reads: its `--umodel` commands export them too).
`python build_ai_anims.py --effects "<CookedPC>" ..\assets` brings sets already
built up to date with their effects' last field (whether the game turns each off
with its clip) without building them again;
`build_sounds.py` reads the soldiers' lines from the game's dialogue packages
(`WL_DX_SLDA01.xxx` and the rest, in `CookedPC`).

### Sound effects

`tools\build_sounds.py` reads the game's SoundCues straight from the packages
(the same ones the map and weapons come from), decodes their recordings (FMOD
banks holding MP3 audio) to `.ogg`, and writes:

```
assets\sounds\*.ogg          3415 recordings, about 76 MB
assets\sounds\sounds.txt     each cue's layers, timing, volume, pitch and variations
```

Each cue also gets the game's mixer category (its root node's `Category`: a
`category` line), which decides which of the Audio settings' volumes it follows:
`DX_DRY` is dialogue (Dialog Volume), `MUSIC_DRY` music (Music Volume) and the
`SFX_` categories everything else (FX Volume).

The categories are the game's mixer's (`SND_SoundConfig_p.SoundMixerProperties` in
`HM_Engine.xxx`, written at the top of `sounds.txt` as `mixcategory` lines, parents
first): a tree from `Master` down, `MASTER_WET` over `SFX_WET` and `DX_WET`,
`SFX_WET` over `SFX_WET_COMBAT` and the rest, each category's volume scaling those
under it. A cue can turn on one of the mixer's presets as it plays (its root node's
`SoundMixerPreset`: a `mixer` line, `local` if only the local player's own copy
does, `PlayMixerPresetOnlyIfLocalPlayerSound`), and a `preset` line gives each
preset's fade in, fade out, how long it stays on (a Duration of 0 or -1: while the
sounds that turned it on play) and the categories it turns down, in dB:

```
preset GLB_DX_DUCK_9DB 0.15 0.5 -1 MUSIC_DRY -9 SFX_DRY -9 SFX_WET -9 SFX_WET_BOSS -3
```

is what every one of the announcer's lines turns on: the effects and music 9 dB down
under the line, the voices (`DX_`) as they are. Presets on at once each turn their
categories down. A `channels` line is a preset a category turns on itself while at
least that many of its sounds play (`ChannelCountSoundMixerPreset`: eight robots'
shots turn the ambience down). `src\mixer.rs` plays them; only their volumes, not the
filters, reverbs and pitch they also set.

A cue whose root node's `SoundParameter` is `SOUND_DISTANCE` gets a `distance` line
for each layer with a volume curve: the layer's volume by how far from the camera
the cue plays (metres). That is how the game's guns sound close up near the camera
and distant past it: the Photon Eliminator's `SHOOT` (the Photon Burst Rifle's and
the enemy soldiers' shot) plays its close-up burst within 5 to 7.5 m and its
`MID_DIST_02` shot from there out. A sound heard where you are (your own) takes the
curves' nearest end.

A layer the game sends to speakers of its own choosing gets a `pan` line: its front
left, front right, centre, back left and back right levels in dB (a plain layer's
are the fronts at 0 and the rest silent, and gets none), and `2d` for a layer always
heard flat or `smart` for one heard flat when it is the player's own (the cue's
`SpatializationType`; a layer placed round the player, `k3D`, uses none of them).
The demo has two speakers: it plays a layer in its centre or back speakers in both,
as loud as ever, and leaves out a layer in none, as a car's or a tank's boost
exhaust is for its driver.

Besides its own list, it takes the cues the classes' characters name in their files
(`assets\models\<class>\<class>.txt`: each vehicle's engine, boost, tyres, crashes,
jumps, landings and death, each transformation's sounds, Snarl's and Slug's
footsteps, tails and growls, Grimlock's feet, the jets' hover, boosters, climbs and
dives, and each class's robot's footsteps, jumps, landings, body falls and sword
swing: see *The Infiltrator's characters*, *The Destroyer's characters*, *The Titan's
characters* and *The Scientist's characters*), so build the characters first.

The menu's sounds and music come the same way: the button sounds are cues in
`BL_HUD_INTERFACE.xxx` whose recordings are in `Startup_int.xxx`, and the menu music
is the cue `MP_FRONT_END` in `UI_FrontEnd_m.xxx`, which plays the Transformers theme
(streamed from `WL_MX_GLOBAL.xxx`) and then fades into "Metroplex Revealed" (from
`WL_MX_L02.xxx`, stored as 5.1 and mixed down to stereo). Music cues also get
`envelope` and `loop_end` lines in `sounds.txt` (see the script). The music plays on
under the Customize screen at half its volume (`CUSTOMIZE_MUSIC_LEVEL` in
`src\menu.rs`), dipping as the menu fades to black into it and coming back up as the
menu returns. The Audio menu's sliders play what the settings movie plays as they
move: `SFX_SLIDER_VOLUME` (a beep) for FX Volume, `DX_SLIDER_VOLUME` (a line of
Optimus Prime's, `DX_OPRIME_LK003118` in `Startup_int.xxx`) for Dialog Volume and
`MX_SLIDER_VOLUME` for Music Volume: a sting from the campaign's score
(`L02A_09_STNG_PLAZA_END`, streamed from `WL_MX_L02.xxx`) whose volume curve fades
it out after a second and a half. The demo follows that curve on a cue played once,
as it does every one-off cue's whose volume curve shapes its sound (88 others,
written as `envelope` lines; `FADED_CUES` in the script names those to keep whatever
their curves): the large enemies' gun among them (`BL_WPN_GUN_AI_TITAN.SHOOT`, whose
curve cuts its recording's long ring off after 0.4 s). The others play their
recordings as they are.

Each cue fades with distance from the camera as the game's sound engine fades it:
at full volume out to its root node's `DistanceMin`, then by the inverse of the
distance (near / (near + `RolloffFactor` x (distance - near)): half the volume, 6 dB
down, at twice the near distance with the usual factor of 1, a third at three times),
silent past `DistanceMax`. No more than its `MaxConcurrentPlayCount` copies of a cue
play at once, and its `InstanceLimiting` decides which goes when one more would play:
the farthest from the camera (the game's default; a new one farther than all those
playing doesn't play), the oldest, or the newest (the new one doesn't play). A cue
that leaves a setting out gets the engine's default for it, as the game does: -6 dB,
4 m, 64 m, 5 copies and a rolloff of 1 (`build_sounds.py` writes them all into each
`cue` line). The demo used to fade them in a straight line from the near distance to
the far one, give a cue that left them out 0 dB, 35 m, 350 m and 16 copies, play
every one-off cue's recording without its volume curve, and always cut the oldest
copy. That let the large enemies drown out everything else: the Heavy Gunners' and
Titans' machine guns (`BL_WPN_GUN_AI_TITAN`) are fired with no set burst length (their
`HeavyGunnerMG_WEPDATA` and `TitanHeavyMG_WEPDATA` leave the bursts at none, so their
AI lets go of the trigger every round or two, as the game's does), and each let-go
plays the gun's end-of-burst ring, `SHOOT_TAIL`. The game plays it at -6 dB, silent
for its first 85 ms and fading in over a quarter of a second, five at most; the demo
played it whole at 0 dB, sixteen at once, 13 dB louder than the shots themselves.
Played as the game plays them, the two guns of a Titan and a Heavy Gunner firing at
Jazz from 20 m are about 9 dB quieter (worked out from the sounds a test match asked
for).

The Nucleon Charge Rifle's scope holds the game's looping cues on rather than
playing them once (`Sfx::hold` in `src\sound.rs`): `ZOOM_LP` for as long as the scope
is up (the zoom's whine, then a hum with crackles of static that only sometimes
play), `ZOOM_CHARGE_LP` while it charges (two tones rising in pitch) and
`ZOOM_CHARGE_COMPLETE_LP` once it is fully charged (a power-up, then a second and a
half of beeps). Cues like these get each layer's volume and pitch curves (the game's
`VolumeCurve` and `PitchCurve`: `envelope` and `pitch` lines), how often a layer
stays silent and how late it may start (`ChanceToPlayNone`, `DelayMin`, `DelayMax`:
`random`) and where the cue loops back to (`LoopStart`: `loop_start`); the demo
plays them that way and fades a held cue out over a tenth of a second when it is
let go. Its other sounds are ordinary cues (`BL_WPN_GUN_NULLRAY`, all in
`L03b_CutAndRun_BASE_m.xxx`), as are the Path Blaster's (`BL_WPN_GUN_BATTLE_PISTOL`,
the game's "Battle Pistol": its shot, low-ammo shot, sights, reload, equip, holster
and change hands), and the Riot Cannon's (`BL_WPN_GUN_FUSION_CANNON`, the game's
"Fusion Cannon": its shot, sights, reload, equip, holster and change hands, its
round's flight loop `TRAIL_LP` and its blast `EXPL_IMPT_WORLD`, in
`L03b_CutAndRun_BASE_m.xxx` and `all_shared.xxx`; its own sound package,
`WL_GUN_FUSION_CANNON.xxx`, is empty), and the Orbital Pistol's
(`BL_WPN_GUN_TRACER_DART`, cooked into the multiplayer maps, `MP_Array_BASE_m.xxx`:
its shot, which is its low-ammo shot too, its sights coming up and going down,
reload, equip, holster and change hands; its shots strike with the energy impacts,
and its empty click is the ballistic one). The Titan's weapons' sounds are cooked
into `all_shared.xxx`: the X18 Scrapmaker's (`BL_WPN_GUN_PULSE_RIFLE`: its wind-up,
held from the pull until its first round and playing on to its end if you let go;
its shot and low-ammo shot over its loop, held while it fires; its tail; sights,
reload, equip, holster and change hands), the Energon Harvester's (the shotgun's,
`BL_WPN_GUN_SHOTGUN`), the E.D.K. TechVolt's (`BL_WPN_GUN_TECH_VOLT`: the beam's
loop, held while it fires and fading out over a quarter of a second as it stops,
its last crackle and its crackle on an enemy, reload, equip, holster and change
hands, with the repair beam's sights and wall impacts, `BL_WPN_GUN_REPAIR_WPN`) and
the Corrosive Slime Cannon's (`BL_WPN_GUN_SLIME_CANNON`: its shot, reload, equip and
change hands, its round's flight loop and its splash, and the grenade launcher's
dry detonation, `BL_WPN_GUN_GRENADE_LAUNCHER`, as it clicks empty). The Gear
Shredder's are cooked there too (`BL_WPN_GUN_SAW_BLADE`: its charge's loop, held from
the first blade loaded to the shot; the blades loading; the shot by how many blades;
its reload; its blades' flight loop, bounce and impact; with the Neutron Assault
Rifle's equip, holster, change hands and sights, as its weapon mesh has them). So are
the Destroyer's: the Thermo Rocket Cannon's (`BL_WPN_GUN_ROCKET`: its shot, the
rocket's flight loop and blast, sights, reload, equip, holster and change hands, with
the HUD's lock tone, `BL_HUD_INTERFACE.HUD_TARGET_LOCK`), the A-4 Pulsar Cannon's
(`BL_WPN_GUN_PLASMA_CANNON`: its shot, its round's flight loop, its fuse once it
sticks and its blast, the detonation, reload, equip, holster and change hands, and the
grenade launcher's dry detonation) and the Chaos-Rift Combustor's (the grenade
launcher's, `BL_WPN_GUN_GRENADE_LAUNCHER`: its shot, reload, equip, holster and change
hands; the bomb's flight loop and blast and the clusters' flight loop, fuse and blasts,
`BL_WPN_GUN_TANK_CANNON`; the clusters' bounce, `BL_WPN_GRENADE`). A held
cue now fades out over its gun's `LoopingFadeOutTime` where the game gives one
(`Sfx::hold_fading`).

The announcer's "Escalation" is the dialog cue `BL_DX_SWITCHBOARD.SC009019` in
`ESC_Gladiator_BASE_m.xxx` (what the announcer's sound event set plays for
`MP_GameTypeEscalationDialog`). It holds a line for each side, Optimus Prime's for
the Autobots and Megatron's for the Decepticons, and the game plays the player's
side's. The script writes each as a cue of its own (`DIALOG_SPEAKERS`): Optimus's
under the cue's name, Megatron's as `BL_DX_SWITCHBOARD.SC009019_MGTRON`, both
English recordings from `ESC_Gladiator_BASE_m_LOC_int.xxx` (`DX_OPRIME_LK009019` and
`DX_MGTRON_LK009020`). `src\menu.rs` plays Megatron's on the Decepticon maps
(Downfall and Oblivion) and Optimus's on the others when the game starts
(`GAME_TYPE_DIALOG`, `GAME_TYPE_DIALOG_DECEPTICONS`), together with the
match-start music `BL_LVL_MP.MX_MATCH_BEGIN` (`GAME_TYPE_MUSIC`, in `all_shared.xxx`):
a second in, one of 13 stingers streamed from `WL_MX_L01.xxx` to `WL_MX_L06.xxx`.

Escalation's waves (see *Escalation's waves*) bring the announcer's wave lines, each
written for both sides the same way (`DIALOG_SPEAKERS`): the countdown's "Next wave
begins in" and "Five" to "One" (`BL_DX_SWITCHBOARD.SC017783` to `SC017793`), "Wave
complete" (`SC017781`), and "We are 3 kills from victory!" and "We are 1 kill from
victory!" (`SC019361`, `SC019363`). With them come the stingers as a wave begins and is
won (`BL_LVL_MP.STNG_ESCALATION_WAVE_BEGIN`, `STNG_ESCALATION_WAVE_OVER_SUCCESS`; the
lost wave's, `STNG_ESCALATION_WAVE_OVER_FAIL`, was already in), and the scan as the
last enemies' markers show (`BL_TRANS_POWER.ORBITAL_BEACON_ONLINE_SCAN`). The
countdown's beep is `BL_LVL_MP.MP_COUNTDOWN_BEEP`.

To rebuild it you need Python and `ffmpeg` (with libvorbis, which the usual Windows
builds include) on your PATH, and `L03b_CutAndRun_BASE_m.xxx`, `mp_shared.xxx`,
`ESC_Gladiator_BASE_m.xxx`, `BL_HUD_INTERFACE.xxx`, `Startup_int.xxx`,
`UI_FrontEnd_m.xxx`, `WL_MX_GLOBAL.xxx`, `WL_MX_L01.xxx` to `WL_MX_L06.xxx`,
`ESC_Gladiator_BASE_m_LOC_int.xxx`, `UI_CustomCharacter_m.xxx` and `WL_AMB_BED.xxx`
(the Customize garage's ambience) and `all_shared.xxx` (the
Photon Burst Rifle's and the Scatter Blaster's sounds, the weapon pick-up and
dry-fire sounds and the newer weapons' sounds) from
`TransGame\CookedPC`:

```
cd Documents\foc_esc\tools
python build_sounds.py "C:\...\Transformers Fall of Cybertron\TransGame\CookedPC" ..\assets
```

The list of cues is `CUES` at the top of the script; the names the demo plays are
the `SND_*` constants and the `sound_*` fields of `WEAPONS` in `src\main.rs`, and
`SFX_VOLUME_DB` in `src\sound.rs` sets the overall volume. Without
`assets\sounds` the demo runs silent.

### Firing effects

The game's weapon effects are Unreal 3 particle systems (for example
`AssaultRifleArm_MuzzleFlash_FX`, `AssaultRifle_VEH_Tracer_FX` and `_Squib_FX`, the
Subsonic Repeater's `Subsonic_Repeater_*_FX`).
Bevy has no particle system of that kind, so `src\fx.rs` rebuilds each one from
sprites: the same textures, blend modes and burst counts, and lifetimes, sizes,
speeds and colours read from the particle systems' data, plus the weapons' muzzle
lights (`TnWeaponEffectLight`). The textures are in `assets\textures\fx`; to export
them again, run umodel on `L03b_CutAndRun_BASE_m.xxx` for each texture, e.g.

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx L03b_CutAndRun_BASE_m.xxx FireSpears_CLR Texture2D
```

(the list of names is `TEXTURES` in `src\fx.rs`). Without them the effects still
play, as plain glowing sprites.

Tracers follow the tracer particle systems (`BULLET_TRACER` and `SUBSONIC_TRACER` in
`src\fx.rs`): the rifle and car guns throw 5-10 fiery streaks, 9-18 m long, at
120-150 m/s; the Subsonic Repeater two thin cyan bolts and a glowing squiggly trail
at 200 m/s. Each streak
starts at the muzzle and ends where the shot struck. The rifle's streaks are drawn
as the game draws its sprites, centred on their particles, and stretch from 0.3 to
twice their length in the first eighth of a second before shrinking (the system's
`SizeMultiplyLife`), so their heads race out to the target at some 300 m/s. Very thin bolts are drawn at
least about two pixels wide so they stay visible at a distance.

The Subsonic Repeater's effects are `Subsonic_Repeater_MuzzleFlash_FX`,
`_Tracer_FX` and `_Squib_FX` (`FX_OS_Weapons_p`), on its weapon mesh's
sockets (`SUBSONIC_SOCKETS` in `src\main.rs`). Its rounds all leave its upper barrel
(`MuzzleFlash`), but the flash plays at the lower one too (`MuzzleFlash2`), the
light at the upper one only. The flash is six caps of white-blue squiggles
(`Ion_Cannon_MuzzleFlash_Cap_CLR`) blowing out to three times their size, two
starbursts swelling to nine times theirs and fading, all on the game's cap mesh
(`MuzzleFlash_Cap_STAT`: four planes crossed along the barrel, 2.24 m long, built
in `src\fx.rs`), and a blue glow, all gone within a tenth of a second or so; and
six copies of the Ion core mesh (`Ion_Cannon_MuzzleFlash_STAT`, whose four crossed
fins each carry one of the comet streaks of `Ion_Cannon_MuzzleFlash_Core_CLR`)
shooting forward from the barrel, growing from a tenth to twice their length while
their width puffs out to three times and back, within 0.03-0.1 s. In the tracer,
six thin spinning copies of the core trail behind the round and stretch to about
45 times their length as it flies, with the two thin cyan bolts. Where a round
strikes, one or two dark spears lie out along the surface and a dark burst swells
facing the camera (`4x4_Bursts_CLR`, which the game uses as a mask:
`tools\build_fx_textures.py` turns it into `4x4_Bursts_Mask_CLR`), then a blue
flash, a burst of white-blue bits and nine blue-white sparks; on walls it leaves
the rifle's bullet hole with its rim glowing cyan (the decal's colour 0.5, 3, 3).
Like the rifle it throws out a shell (`Sm_Shell3_FX`) for one shot in four, from
the port under its drum (`ShellEject`), and steams from its `ReloadFX` socket as it
reloads (`ReloadSteam_FX`, the rifle's steam at half the rate). Not rebuilt: the
heat-haze rings along the tracer's path and the flash's and impact's glows that
darken or brighten what is behind them.

The core mesh is in `assets\models\fx\ion_core.glb`. To build it again:

```
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=fx L03b_CutAndRun_BASE_m.xxx Ion_Cannon_MuzzleFlash_STAT
python tools\build_fx_meshes.py fx\L03b_CutAndRun_BASE_m\StaticMesh3\Ion_Cannon_MuzzleFlash_STAT.pskx assets\models\fx\ion_core.glb
```

Without the file, the Subsonic Repeater uses flat glowing sprites for the core
instead.

Without the characters' data (theirs: *Vehicle effects and heat haze*, below),
`assets\models\vehicle.glb`'s Jazz has the demo's own versions of the car's effects,
after its `JazzVehicleForm`'s (`FX_Navigation_p`): the jump blast (`CarJump_01_FX`),
the hover jets (`CarHover_A_01_FX`), the boost flames (`Booster_Jazz_FX`) and the
landing sparks and dust (`Vehland_Low_Med_FX`, `Vehland_High_XSm_FX`, and the hover
car's `Vehland_Low_XSm_FX`), at the car's sockets. The hover jets are mesh particles on
`Boostermesh_03`, a flared cone, in `assets\models\fx\booster_cone.glb`:

```
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=fx L03b_CutAndRun_BASE_m.xxx Boostermesh_03
python tools\build_fx_meshes.py fx\L03b_CutAndRun_BASE_m\StaticMesh3\Boostermesh_03.pskx assets\models\fx\booster_cone.glb --along x
```

Their textures (`LightBeam_Falloff_01_CLR`, `lightningring_01_CLR`, `Sparks_01_CLR`,
`Spot_CLR`) go in `assets\textures\fx` with the others.

The Photon Burst Rifle's effects are `Photon_BurstRifle_MuzzleFlash_Fx`,
`_Tracer_Fx` and `_Squib_Fx` (all_shared): a blue streak, star and flames at the
barrel with a pale blue light, ten deep-blue streaks at 320-350 m/s, and a streaky
blue star with sparks where it hits. Their textures are exported from `all_shared.xxx`
(their pixels are in `FX_OS_Common_Textures_p.xxx` and `FX_Textures_p.xxx`) and
copied by `tools\build_fx_textures.py`, which also splits the channel-packed
`StarBurst_Greypak_01_CLR` into the two images the effects use:

The Scatter Blaster's are `Scatterblaster_MuzzleFlash_FX`, `_Tracer_FX` and
`_Squib_FX` (all_shared), on its weapon mesh's sockets (`SCATTER_SOCKETS` in
`src\main.rs`): a burst of fire at the barrel's `MuzzleFlash` (the four flames of
`MuzzleCore_ClusterBomb_CLR`, which the game puts on a cap mesh, drawn here as
crossed sprites that blow out wide and then stretch along the barrel), sparks and
a little rolling smoke with a warm light; per pellet its tracer: three FireSpears
streaks (one of `Bolt`'s, two of `Bolts_Dup`'s), 12-20 m long at 120-150 m/s with up
to 7 or 10 m/s of sideways drift, each centred on its particle and stretching from
0.3 to 2 times its length and back as the rifle's do, up to three dark smoke streaks
flying with them (`Smoky_Bits`) and a dark and a faintly glowing smoke puff thrown
along the line at 80-100 m/s (`Smoky_Puffs_Dark`, `Smoky_Puffs`), all cut off where
the pellet struck; and where a pellet hits, a warm flash and a lingering glow on the
surface, bouncing sparks, two embers and two puffs of dark smoke. Each of these
emitters has no more particles out at once than the game cooks for it (its
`PeakActiveParticles`: the size the game gives its particles' array, which a burst
asking for more never outgrows): `Bolt` and `Bolts_Dup` ask for 5-10 streaks and
keep 1 and 2, the muzzle's smoke asks for 12-15 puffs and keeps 5 and 4, so a shot
shows the few streaks and the light smoke of the game's videos, not the fan of a
hundred streaks and the cloud of smoke they would make. The shotgunners' shotgun
effects and the Fireblast's (`TurretArm_MuzzleFlash_FX`, `Trail_ScatterBlaster_SP_FX`,
`Squib_ScatterBlaster_FX`, `Squib_ScatterBlaster_SP_FX`, the game's own particle
systems: `src\levelfx.rs`'s `PEAK_CAPPED`) keep to theirs the same way, and so does a
tank shell's blast (`TankCannon_ImpactExp_FX`), whose smoke emitters ask for 10 to 25
puffs of 10 to 90 m and keep 2 to 7. With every
shot `Med_Shell2_FX` plays at its `ShellEject`: a shell 2.3 times the rifle's
(`shell2_STAT`), its open end glowing cyan (the casing material's mask, lit in
Jazz's energon colour), and grey steam out of the shell port for 1.5 s. As it reloads, `ReloadSteam_Med_FX`
and the paler `ReloadSteam_Med02_FX` steam from its `ReloadFX` socket by Jazz's
wrist. `MuzzleCore_ClusterBomb_CLR` is the one new texture they need (the others
are shared with the effects above; the steam is the rifle's `SmokeThin_CLR`, below).

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx all_shared.xxx Streak_CLR Texture2D
(likewise StarBurst_Greypak_01_CLR, Light_Star_Streaky_CLR, MuzzleFlash_Light_CLR and MuzzleCore_ClusterBomb_CLR)
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx L03b_CutAndRun_BASE_m.xxx SmokeBall_CLR Texture2D
(likewise Smoke_Lrg_CLR, SmokeSpear_CLR, MetalDebris_CLR and 4x4_Bursts_CLR)
python tools\build_fx_textures.py fx\all_shared\Texture2D fx\L03b_CutAndRun_BASE_m\Texture2D assets
```

The Nucleon Charge Rifle's are `NucleonRifle_MuzzleFlash_FX`, `_Tracer_FX` and
`Squib_SniperRifle_FX`, on its weapon mesh's sockets (`NUCLEON_SOCKETS` in
`src\main.rs`): at the barrel six caps of orange flame (the four flames of
`MuzzleFlash_Sparks_CLR` on the cap mesh) shooting out along it for a tenth of a
second, a big cloud of fiery smoke rolling out up to 6 m ahead and five glowing puffs
in it that flare white-hot as they leave, with a red light (the game's brightness 10:
the floor round Jazz goes red with every shot); three cyan bolts, 7-13 m long, racing
out at 900-970 m/s and cut off where the round struck, and see-through rings hanging
round the shot's last few metres, swelling and fading in a third of a second (the
game draws them as heat haze; here they are pale rings); and where it strikes, white-hot
flares and long white streaks thrown out along the surface's normal, an orange glow,
two showers of orange sparks, a flash of bits, a burst of fire a twentieth of a second
later (`MuzzleCap_ClusterBomb_CLR`, swelling to some 2 m and back), metal chips, a ball
of fire that flares and smoulders for a second, a glow left on the surface for
nearly two and grey smoke. Fully charged, `NucleonRifle_Charged_FX` lets a red ring
out round the barrel 20 times a second, each drifting back along it for half a
second. Each shot throws out the Scatter Blaster's big shell with its steam
(`Med_Shell2_FX`), and it reloads with the Subsonic Repeater's steam
(`ReloadSteam_FX`). Its new textures:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx L03b_CutAndRun_BASE_m.xxx Ring_CLR Texture2D
(likewise MuzzleFlash_Sparks_CLR, MuzzleCap_ClusterBomb_CLR, Spark_Base_CLR and smokeball_01_CLR)
python tools\build_fx_textures.py fx\L03b_CutAndRun_BASE_m\Texture2D assets
```

Their pixels are in `FX_OS_Common_Textures_p.xxx` and `FX_Textures_p.xxx`, which have
to be in `CookedPC`. Not rebuilt: the bolts' dark smoke trails and squiggles, the chips
of the struck surface's own material and the impact glow's brightening of the surface
(here it is added light).

The Path Blaster's are `PathBlaster_MuzzleFlash_FX`, `_Tracer_FX` and `_Squib_FX`
(`FX_OS_Weapons_p`), on its weapon mesh's sockets (`PATH_SOCKETS` in `src\main.rs`).
Its muzzle flash has the Nucleon Charge Rifle's three emitters (the flame caps, the
fiery smoke and the glowing puffs) and three of its own on another cap mesh, a dish
1.54 m across that faces out along the barrel (drawn here as flat sprites facing the
same way): a white-hot flash (`MuzzleFlash2_CLR`, the game's colour 10, 7, 5, swelling
to three times its size), a fire burst spinning as it blows out to three times its
size and fades (the red of `MuzzleCap_ClusterBomb_CLR`, tinted orange) and a fiery iris
ring that comes up as it swells (`PathBlaster_MuzzleCore_Mat`, which mixes
`SpaceBridge_Iris_CLR`, `TankMuzzleFlash_CLR`, `MuzzleCap_ClusterBomb_CLR` and
`MuzzleCap_RocketTurret_CLR`: `tools\build_fx_textures.py` mixes them the same way into
`PathBlaster_MuzzleCore_CLR`), with an orange light at the game's default brightness of
25, the strongest of the guns'. Its tracer is three bolts, 7-13 m long and red-hot
(colour 50, 2, 2: white at the core), at 180-510 m/s. Where a round strikes: a burst
of fire flaring white-hot, fire lingering for half a second, dark smoke, a spray of
golden sparkles (`ember_clip_CLR`, seven big sprites stretched along their flight and
slowing), flames licking up (`Flame_CLR`) and, on walls, a scorch mark 1.25-1.6 m
across (`blastmark_glow_CLR`'s burn, as `Blastmark_CLR`) that lasts 6-12 s, fading
over the second half. Each shot throws out the Scatter Blaster's big shell with its
steam (`Med_Shell2_FX`) behind Jazz's wrist, and it reloads with the Scatter Blaster's
paler steam (`ReloadSteam_Med02_FX`). Its new textures:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx all_shared.xxx SpaceBridge_Iris_CLR Texture2D
(likewise TankMuzzleFlash_CLR, MuzzleCap_RocketTurret_CLR, MuzzleCap_ClusterBomb_CLR, ember_clip_CLR, Flame_CLR and blastmark_glow_CLR)
python tools\build_fx_textures.py fx\all_shared\Texture2D assets
```

Not rebuilt: the impact's heat haze and its tracer's smoke puffs left at the muzzle
(as the Nucleon Charge Rifle's), and the scorch mark's brightening of the surface
round its glowing specks.

The Neutron Assault Rifle's effects are its weapon mesh's
(`WEP_AssaultRifleArm_p.Def.AssaultRifleArm_WEPMESH`): `AssaultRifleArm_MuzzleFlash_FX`
at whichever of its two barrels fires (`MuzzleFlash`, `MuzzleFlash2`, on Jazz's
`R_Arm03_Elbow_XB`; `GunSockets` in `src\main.rs`), with its light; the car gun's
`AssaultRifle_VEH_Tracer_FX` and `_Squib_FX`; `Sm_Shell3_FX` at its shell port
(`ShellEject`), which throws out a shell for one shot in four (its open end glowing
cyan) and lets steam out of the port for one in five; and `ReloadSteam_Med_FX`
there as it reloads. The flash is three pairs of crossed flame
sprites standing in for the six caps of its `Flash_Blast`, which burst out wide for
the first tenth of their 0.1-0.13 s and then draw out along the barrel to twenty
times their first length (its `SizeMultiplyLife`), with a glow, nine spark streaks
that start seven times their length and fly out at three times their speed for the
first part of their life, four or five dark smoke puffs and a faint yellow haze. Where
a round hits, the burst plays along the surface's normal (as the game's
`EffectOrientation` does), its flash a twentieth of a second after the rest, and on
walls it leaves a bullet hole: the squib's decal emitter, a metre-wide decal of two
holes (`ENV_ASH_BulletHoleDecal_CLR`) whose rims glow orange-hot
(`ENV_ASH_BulletHoleDecal_CLR2`, times the decal's colour 5, 3, 1) and cool as it
fades over 0-7 s. The reload steam is a cloud of grey-brown wisps (`SmokeThin_CLR`)
puffing out through the reload, each swelling from a quarter of 1.2-2.5 m to 1.75
times it over 1.5-2 s. These textures are made by `tools\build_fx_textures.py` too:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx FX_Textures_p.xxx SmokeThin_CLR Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx ENV_ASH_interior1_p.xxx ENV_ASH_BulletHoleDecal_CLR Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=fx ENV_ASH_interior1_p.xxx ENV_ASH_BulletHoleDecal_CLR2 Texture2D
python tools\build_fx_textures.py fx\FX_Textures_p\Texture2D fx\ENV_ASH_interior1_p\Texture2D assets
```

Without them the steam and the bullet holes are left out. Not rebuilt: the
flash's black smoke caps and darkening glow, the impact's darkening flashes and its
metal decal, the steam's heat haze and jets, and the shells' glow and smoke trail.

Grenades go off with their own explosion, the grenade's `ExplosionEffect`
(`FX_WPN_EXP_Grenade_p.FX.Grenade_EXP_FX`, from its projectile mesh
`WEP_Grenade_p.GrenadeSkill_PROJMESH`): a big orange glow crossed by thin rays of
light, 40 long sparks, a fireball rolling into dark smoke, spears of smoke thrown up
and two bursts of metal debris, with an orange light. The smoke and debris textures
come from `L03b_CutAndRun_BASE_m.xxx` (above); `build_fx_textures.py` turns their
material masks into plain sprite images. The scorch mark it leaves, its heat blur
and its blast-wave mesh aren't rebuilt.

An effect on one of a gun's sockets plays at its own size, whatever that socket's
scale: the game attaches it with the effect's own scale
(`HmParticleEffectManager.PlayEffectOnSocket` hands `AttachComponentToSocketWithOffsets`
its `EffectScale`, 1, in place of the socket's), so a socket's `RelativeScale` is no
part of the effect, its signs neither: a socket mirrored by a negative scale doesn't
turn the effect round (only its `RelativeRotation` turns it). The A-4 Pulsar Cannon's
glow shows it: its ball of plasma sits 50 units along the effect's -X from the gun's
`IdleFXSocket`, which the left gun's socket turns down the barrel by its rotation and
the right gun's points down it unturned; turned round by the right gun's scale
(-0.7, -0.7, 0.7) as the demo had it, the ball sat back by the elbow instead of on
the gun's front. (The X18 Scrapmaker's barrel smoke's sockets, -0.4 on the right
gun and 0.4 on the left, mirror each other only so too.) Several sockets are
scaled down (the X18 Scrapmaker's muzzles to a tenth, its barrel smoke 0.4, the
Corrosive Slime Cannon's reload steam and idle smoke 0.7, the right-hand Thermo Rocket
Cannon's exhaust, reload smoke and steam 0.7, the A-4 Pulsar Cannon's idle glow 0.7, the
reload steam of the Subsonic Repeater, the Nucleon Charge Rifle and the Path Blaster);
their effects are as big as on any other socket, which is how the game's videos show
them (the X18's muzzle flash a big star about two or three times the barrel housing).

### Vehicle effects and heat haze

Every car's and truck's effects are the game's particle systems on its sockets, each
vehicle's own (its `TnCarFormBlueprint`'s or `TnTruckFormBlueprint`'s `HoverFX`,
`BoostFx`, `JumpFX` and landings), played as `TnCarForm` plays them (`src\car.rs`'s
`vehicle_effects`, as the tanks' and jets' are): Jazz's hover jets (`CarHover_A_01_FX`,
the glowing pulse, rings, sparks and rays under each wheel) while the car hovers, from
the moment it changes, and its boosters (`Booster_Jazz_FX`: three cones over each
exhaust) while it drives on its wheels; its jump blast (`CarJump_01_FX`) and its
landings (`Vehland_Low_Med_FX`, `Vehland_High_XSm_FX` after a second or more in the
air, the hover car's `Vehland_Low_XSm_FX`). The hover jets are sized by how hard each
one pushes (`TnCarForm.Hovering.UpdateFx`'s `Size`: the car's acceleration against
gravity along the jet, over 30 m/s^2, and the turn it would give against the car's
angular acceleration, over 6 rad/s^2, at most their full size), so they burn full as
the car floats, die down as it falls and flare as it lands. Their colour is the
driver's energon colour (`TnVehicleFxPlayer.Play`'s `Color`, his faction's in a match,
as his glow is: the tanks' and jets' too), and the boosters' turns yellow while a jump
on the wheels boosts the car, fading back over 0.7 s (`Driving.UpdateBoostFx`). Cars
and trucks show none in the Customize garage, as the game's don't.

The Dinobots' landings (Grimlock's, Slug's and Snarl's: the walkers) play on the floor
under their sockets. The sockets their blueprints name for them are up inside their
bodies: Grimlock's `LandFX` is the tanks' list, its `TargetingSocket` (his lock-on
point, in his chest) and two hover sockets his model hasn't got, and Slug's and
Snarl's sockets are 1.5 m up their bodies' bone. Played there, Grimlock's dust burst
out of his chest; the landing effects are bursts of sparks and rings of dust off the
ground (every other vehicle's sockets are low on it, near the floor).

Their heat haze is their materials' **distortion**. Materials the game draws with
one (`bUsesDistortion`: the hover jets' and boosters' cones, rays and rings, shock
rings, the Diffraction Barrier's shield, some blasts, tracers and the melee trail)
shift the picture behind them: `tools\ue3\fxmat.py` translates their `Distortion`
input too, and the effects' shader (`FX_DISTORT`) moves the scene behind by it as
Fall of Cybertron's distortion pass does: its distortion shaders (in the packages'
shader caches) keep four times the shift over 255 in eight bits (at most 63.75 each
way, any under a twelfth left out) and move the picture by a 255th of the screen for
each unit of it (V the other way). The game adds every layer's shift into one buffer
and then moves the picture once. Here each distorting material draws its
distortion as a part of its own, just before the rest of it (`FX_DISTORT_PART`): the
scene's copy moved by its shift, less the copy, added to the picture as far as no
distortion drawn before it there has moved it yet. The picture's alpha keeps count
(it starts at 1, everything opaque leaves it so, and each layer uses up a share
of it as its shift nears a 255th of the screen; a part drawn last puts it back to 1:
`src\levelfx.rs`'s `distort_blend` and `AlphaRestore`), so however many layers
overlap, a place moves about as far as the furthest of them, as with the game's
single move, where adding each layer's move piled the differences up. A modulating
material's bright part adds the scene as its shift moves it. And the rest of a
distorting material is clipped where its opacity is under a 255th
(`FX_OPACITY_CLIP`), as the game's compiled shaders for every material with both a
Distortion and an Opacity do (`texkill` on the opacity less 1/255).

`tools\build_weapon_fx.py` reads the cars' and trucks' systems with the weapons', and
`tools\build_class.py --fx` writes each vehicle's effects (its `vfx` lines) into its
class's file without building the class again:

```
cd Documents\foc_esc\tools
python build_weapon_fx.py "C:\...\TransGame\CookedPC" C:\temp\fxmesh ..\assets
python build_class.py --fx infiltrator "C:\...\TransGame\CookedPC" "C:\...\TransGame" ..\assets
python build_class.py --fx destroyer "C:\...\TransGame\CookedPC" "C:\...\TransGame" ..\assets
```

`tools\build_class.py --transforms` writes where each class's cars are drawn as they
change form (its `vtransform` lines, measured from the class's `clips.glb` and vehicle
models as built) without building the class again:

```
cd Documents\foc_esc\tools
python build_class.py --transforms infiltrator ..\assets
```

### The robots' dash, jump and landing effects

A robot's moves play the game's effects where his clips have them (their
`HmAnimNotify_PlayEffect` notifies, `src\movefx.rs`), as the clip passes each one's
time, local and online players alike:

- **Dashing** (`Nav_Boost_F/B/L/R` on the ground, `Nav_Air_Boost_F/B/L/R` in the air)
  and **jumping** (`Nav_TakeOff_01`): `CarJump_01_FX` on the boosters at his ankles
  (`BoosterSocket_L/R`) and the soles of his feet (`MeleeSocket_LeftFoot/RightFoot`),
  flames and sparks in his energon colour (the notifies'
  `TnParticleSystemParameterEnergonColor`: his faction's, an Autobot's salmon pink).
  A dash to the side fires the boosters again to stop. (The dashes' clips also
  notify dust as they end, `Long_Fall_Landing_1_FX` on `RootSocket` as a side dash
  touches down, `Long_Fall_Landing_Sm_FX` at the feet as one forward or back ends, a
  Destroyer's or Titan's back dash `Vehland_Low_Med_FX`; the demo leaves it out, as
  the game's multiplayer shows none: `DASH_DUST` in `src\movefx.rs`.)
- **Landing** (`Nav_Land`, `Nav_Land_Hard_01`): dust thrown up at his right ankle
  (`Long_Fall_Landing_1_FX`).

The dust keeps to the game's limits on how much of it is out at once (each emitter's
cooked `PeakActiveParticles`, `PEAK_CAPPED` in `src\levelfx.rs`): its bursts ask for
up to 90 puffs 8 to 10 m across and as many sheets on the ground spreading to 50 m,
and the game keeps 20 (a side dash's and a landing's), 3 or 3 puffs and one sheet: a
thin ring of dust round his feet, gone in a few tenths of a second, not a fog over
the floor.

Each is placed on its socket by its notify's `RelativeLocation` (at the robot's
size) and `RelativeRotation`, carried with the socket as the game attaches it
(`bAttachToMesh`), lit by the level's light where it starts as the robot is (an
effect attached to his mesh takes its light environment: the dust is the tan haze
of the game's videos, not the dark one of an effect the game spawns loose; the sun
lights it only where nothing of the level stands between it and him), and stopped once its clip has played out (`bAutoDeactivate`: the
clip's length and 0.04 s after it starts; what it has out lives on). The Destroyer
and the Titan play their own dash and jump clips (`mp_shared.xxx`'s
`Shared_Truck_ROBO_ANIM`), with their own effects' times; the Infiltrator and the
Scientist play the shared ones (`all_shared.xxx`'s `Shared_ROBO_NAV_ANIM`).

`tools\build_robot_fx.py` writes them (`assets\models\robot_fx.txt`) from the
clips each class plays (its `robotclip` lines say which are its own), straight from
CookedPC; the effects themselves are `tools\build_weapon_fx.py`'s:

```
cd Documents\foc_esc\tools
python build_robot_fx.py "C:\...\TransGame\CookedPC" "C:\...\TransGame" ..\assets
```

### Putting the gun away to sprint

As in the game, a robot puts his gun away while he sprints and while he gives a
teammate a revive (`TnWeapon.ShouldHolster`: his acrobatics' `Sprinting` state, on
the ground and not jumping or falling, or `IsGivingRevive`), and takes it out again
when he stops (`src\main.rs`'s `holsters` and `animate_jazz`):

- **Away**: the gun plays its holster effects (its `Unequip_WEP` clip folding it up
  on the forearm's gun bones, and its holster sound) and is hidden after the game's
  `DetachDelay`, a second. With no gun in his hand, the forearm on its arm comes out
  (`TnArmAttachmentManager`): its forearm set's `ADD_Weapon_Equip_ARM` unfolds it out
  of the elbow, its pieces growing back as the clip's `TnAnimNotify_ScaleBone`
  notifies say, and the shoulder pads play their `EquipUnequip` move as for a weapon
  change. The gun's upgrade parts go with it.
- **Back**: the gun shows and plays its equip effects (its `Equip_WEP` clip and
  sound) while the forearm moves onto the gun's bones and folds away
  (`ADD_Weapon_Unequip_ARM`, its pieces shrinking), hidden as that ends.

Firing or fine aiming ends a sprint (as before), so the gun comes back out, and it
fires once it is in his hand (`TnWeapon`'s `Holstered` state fires nothing). He
can't switch hands while it is away or on its way. Swapping weapons as it comes back
out, the gun it was bringing out goes at once (the forearm's folding away has the
bones the old gun would fold away on) and the new one unfolds. (In the game a reload keeps the
gun where it is; here a reload brings it out, to be seen reloading.) Only a robot
does it: a transformation or a death puts the gun back at once, as the game does.
Online, his copy on the other players' PCs does the same as it sprints.

The forearm clips are each class's forearm sets' (`<set>.psa` in the class's export,
as `tools\build_class.py` reads them for switching hands), written by
`tools\build_arm_equip.py` beside the class's other files
(`assets\models\<class>\arm_equip.glb`, `arm_equip_mirror.glb` for the gun in the
left hand, and `arm_equip.txt` with the clips' scale notifies):

```
cd Documents\foc_esc\tools
python build_arm_equip.py infiltrator <his export folder> "C:\...\TransGame\CookedPC" ..\assets
python build_arm_equip.py destroyer C:\dstx "C:\...\TransGame\CookedPC" ..\assets <his export folder>
```

(The last folder is any export with `RB_Jazz_CHARSKEL_Legs`, the robots' skeleton,
for a class whose export lacks it.)

### The EMP grenade

The Infiltrator's EMP Grenade is the game's `FlashBangAbility`: a `TnAbilityGrenade`
that throws a `TnProjectileFlashBang`. `src\emp.rs` plays it with the numbers in
`MP_Array_BASE_m.xxx` (`WEP_FlashBangs_p`) and `TransGame.xxx`:

- **The ability.** A resource of 15 that refills at 1 a second; a throw takes all of
  it. Press Ability (Q) and, a tenth of a second later, the arc it would fly shows
  (`Grenade_Trajectory_FX` run along a simulated throw, bounces included, with the
  box where it will go off); let go and Jazz plays the throw (`Grenade_Throw_MP`) and
  the grenade leaves his hand 0.3 s later, towards where you were looking as you
  let go. It can't be used in the car; changing form with it held gives it back.
- **The throw.** It is aimed at the first thing the view meets from 5 to 50 m out
  (`TraceRange`) and tossed to reach it in 0.1 to 0.5 s (`FlightTime`, longer the
  further it goes) under the game's gravity.
- **The grenade** falls and bounces (each bounce keeps 0.4 of its speed,
  `BounceDampening`) and goes off 0.25 s (`FuseTime`) after it first strikes
  something. It flies as `FlashBang_Grenade_FX` (a burst of cyan light, sparks,
  crackling lightning and a trail) with its flight loop, and starts its fuse sound
  as it lands.
- **Going off:** `Exp_FlashGrenade_FX` (a flash, crossed rays, a burst ring and the
  glowing blast dome, long sparks, billowing smoke, metal bits and specks) with its
  blast sound. Every enemy within 15 m (`DamageRadius`) with nothing in between is
  knocked out for the time the blast's `RadiusDamageScale` gives at its distance:
  4 s out to 7.5 m, then from 2 s down to none at 15 m. Meanwhile it neither shoots
  nor turns, and the game's `EMPAttack_FX` crackles over it. Teammates aren't
  touched.
- **The upgrades** (the build's ability upgrade): **Short Fuse** halves the fuse,
  **EMP Intercooler** refills it twice as fast, and **Speed Redirector** makes Jazz
  run half as fast again for 3 s after a grenade catches an enemy, with the speed
  boost's trail (`SpeedBoost_Trail_FX`) and sounds.
- **The HUD** shows it as the game does (`robotAbility_mc`, bottom right, in robot
  form): the EMP grenade's icon, the key, and the meter filling round it, small and
  pale while it refills and full size, glowing, once a throw is ready (with the
  game's ready sound).
- **Online**, a thrown grenade goes to the other players' PCs, where it flies,
  bounces and knocks out the enemies the same way.

Its effects are spawned where they are needed, so `src\levelfx.rs` runs them the way
it runs a map's, with what they need besides: trail emitters (ribbons left behind by
a moving source), mesh emitters (the blast dome, `Sphere_03_MESH`), and emitters that
darken and brighten the picture behind them. `tools\build_weapon_fx.py` reads the
five particle systems with their materials and textures from `MP_Array_BASE_m.xxx`,
`all_shared.xxx` and `mp_shared.xxx`, and writes `assets\fx\weapons_fx.txt`, their
materials into `assets\shaders\levelfx.wgsl` (`assets\shaders\levelfx\weapons.wgsl`),
their textures into `assets\textures\levelfx\` and the dome's mesh as
`assets\models\fx\Sphere_03_MESH.fxmesh`, from umodel's export of it:

```
cd Documents\foc_esc\tools
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=C:\temp\fxmesh all_shared.xxx Sphere_03_MESH
python build_weapon_fx.py "C:\...\TransGame\CookedPC" C:\temp\fxmesh ..\assets
```

Its sounds come with the others from `build_sounds.py` (which now also reads
`MP_Array_BASE_m.xxx`), and its HUD icon from `build_hud.py`. The emitters that blur
the picture behind them (`Blur_Dup`) and the screen effect the game plays for a
player caught by an enemy's grenade (`FX_Screen_p.FlashBang_FX`) aren't rebuilt,
since only enemies are caught here.

### The multiplayer melee

Melee (middle click; the right stick's click on a controller) is the game's
multiplayer melee (`src\melee.rs`; `tools\build_melee.py` reads it from the game).
Every class swings its own sword, its melee set's attack (the robot definition's
`MeleeSet`, `TR_Shared_MS_p.MPMeleeSet_<class>`: its `MELEE_WeaponAttack`,
`Melee_Sword_Infiltrator`, `_Destroyer`, `_Titan` or `_Scientist`), at the clip's own
rate (1.27 s), as `TnMeleeManager.PlayMeleeAttack` plays it. What its blow does is
Escalation's: every Escalation character's melee set is the game's shared one
(`TR_Shared_MS_p.SharedMeleeSet`, in the `ESC_` maps' robot definitions), whose sword
attack (`Melee_EnergonSword_01`) does 75 and pushes 75000 forward and 75000 up
(the multiplayer set's does 135 and pushes 20000 and 10000, too little to knock
anyone down), and whose sprinting attack (`Melee_Boost`) does 150 with the same
push (`src\main.rs`'s `MELEE_DAMAGE`, `MELEE_SPRINT_DAMAGE` and `MELEE_MOMENTUM`):

* **the Ion Blade** (the Infiltrator's and the Scientist's swings: only theirs carry
  the notify that brings it out, so the Destroyer and the Titan swing without it, as
  in the game): as the swing starts (its `TnAnimNotify_WeaponMelee`, 9 ms in)
  the gun in his hand puts on its melee attachment, `WEP_IonBlade_SKEL`
  (`TransCustomization.ini`'s `IonBlade`, every specialty's), on the gun's
  `MeleeSocket` (on the forearm's elbow bone, placed and sized for each gun, on
  either arm), and its pieces slide out of the housing (`IonBlade_Equip`, a third of
  a second); as the swing ends they slide back in (`IonBlade_UnEquip`, started its
  0.37 s before) and it comes off (`TnWeapon.EquipMelee`'s `UnequipTime` and
  `HideTime`). Its housing and blade are painted in the character's colours on the
  rocket launcher's textures, as the game's `WEP_IonBlade_MATINST` paints them, and
  a strip along its edge glows red and orange with pulses running through its
  circuitry (`WEP_BLadeAut_MAT`, its graph in `assets\shaders\ionblade.wgsl`);
* **the blow:** 0.15 s in (its `TnAnimNotify_DamageSweep`) a box sweeps the space
  ahead of him for a fifth of a second (the Infiltrator's for a quarter), as the
  game's `TnTraceSweeper` does: centred on the robot's `MeleeSocket_SmallRobot`
  (ahead of him at the height of his chest, his size) moved by the sweep's `Offset`
  turned as he faces (1.1 to 2 m to his right, or to his left with his gun in his
  left hand: `TnTraceSweeper.AddOffset`), its `Extent` 5.4 to 7 m across and 7 to
  8 m high. It strikes each enemy whose body it meets once, for the attack's 75
  (`TnDamageTypeMelee`): the Infiltrator's reaches from 0.6 to 6 m ahead of him, the
  Titan's from 1.5 to 8.5 m. Its momentum is the attack's `HitMomentum` (75000
  along the ground from him to the enemy and 75000 up,
  `TnMeleeManager.ServerProcessMeleeHit`): an enemy it doesn't kill is **knocked
  back** if the push would keep it in the air for over half a second
  (`TnHtnAiPawn.TakeDamage`, over its `Mass`: a soldier's 100 takes it up at
  7.5 m/s, 0.51 s in the air), flying back with it through its knockback's take-off,
  flight and landing animations (`STS_Knockedback`; see *The enemies' AI*), and a
  heavier one is shoved; one it kills dies a melee death
  (`Melee_CONDEATHSET`): once in three thrown back several metres
  (`MeleeRagdoll_DEATH`: twice the momentum and two impulses where it was struck,
  on the bone struck and the lower back), twice its limbs cut away near the blow
  and its body burst from 3 m under where it was struck, its lower back pushed
  where the blade landed (`MeleeDismember01_DEATH`);
* **the lunge:** every swing lunges as it begins, at the enemy under his crosshair
  or with none straight ahead the way he looks (the flight's `Direction`, his yaw:
  `TnFlier.Update` flies along it with no `Target`), Escalation's (`SharedMeleeSet`'s sword attack: `AllowFlight` and its
  `AttackDash`, 2500 for 0.25 s; multiplayer's sets have none). The enemy is the one
  the melee assist picks (`TnTargetPickingTypeMeleeAssist`): within 20 m of him
  (`MeleeAssistRange`), ahead of the way he faces, in sight, its targeting point
  within its picking radius of the crosshair's line (its targetable component's
  `MeleeAssist` picker: 3.75 m for the robots, 5.5 m for the Flyers' jets, 0.8 m for
  an insect form), the nearest to that line. He flies at it at once (`TnFlier`,
  `PHYS_Flying`) at 25 m/s times the swing's speed multiplier, 0.75 (and his
  others), his own run added but never back, for up to a quarter of a second
  (4.7 m), and stops dead as he reaches it (their two collision radii apart:
  `DisableMotionOnContact`) or his blow lands. A further enemy he lunges part of
  the way to; with none he flies the whole quarter of a second;
* he barely moves while he swings, and once the swing has met an enemy (his lunge
  reached it or his blow landed) he stands till it is over; he can't jump or dash,
  and his gun doesn't fire until the swing is over (it carries the blade); the swing
  plays its sound (`SWORD_SWING_LT_02`) and his grunt;
* **a blow that lands** (his sword's or his charging punch's, any damage done:
  `TnPlayerController.ClientNotifyCausedDamage` with `TnDamageTypeMelee`'s
  `AttackerCameraShakeParams` and `AttackerCameraEffect`) shakes his view
  (intensity 1.25 with a roll of 1, falling off over 0.4 s) and splashes energon
  across his screen: the game's `Screen_EnergonBloodsplatter_FX`, played as its
  camera plays it (`TnCamera.SpawnEmitter`: 85 uu in front of the camera, facing it,
  kept there as the view moves; not again while one started in the last 0.4 s is
  still showing), its drips and splats bending the picture behind them and greying
  it as they run. Only on his own screen.

The Infiltrator swings his sword too (`Infiltrator_ROBO_ANIM`'s
`Melee_Sword_Infiltrator`), in place of Jazz's campaign shield bash; sprinting
forward, Melee is still the charging punch (`Melee_Boost`). Online, each PC tells the
others where its player's blow is as it begins (its box and the way he faces:
protocol version 32), and they sweep it there, as they do his shots.

The files, made by `tools\build_melee.py`:

```
assets\models\weapons\ionblade.glb         the Ion Blade: its 17 bones and its skinned mesh, in its two materials
assets\models\weapons\ionblade_mask.ktx2   its edge's circuitry (WEP_IaconMask)
assets\models\weapons\melee.txt            the blade's clips, each gun's MeleeSocket (both arms), each class's swing and blow
```

To make them, export the blade from `mp_shared.xxx` (the script prints the umodel
commands) and build (`..\assets` needs `models\robot.glb`: the blow's socket is on its
skeleton):

```
cd Documents\foc_esc\tools
python build_melee.py --umodel "C:\...\TransGame\CookedPC" C:\blade
python build_melee.py "C:\...\TransGame\CookedPC" C:\blade ..\assets
```

The Infiltrator's sword swing is the one clip of his own in his `robot_clips.glb`
(its length in his class line). An Infiltrator built before it gets it from the
robot clips pass, with his melee set exported into his export folder (the one
`build_infiltrator.py` was run on):

```
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=<his export folder> MP_Array_BASE_m.xxx -obj=Infiltrator_ROBO_ANIM
python build_class.py --robotclips infiltrator <his export folder> "C:\...\TransGame\CookedPC" "C:\...\Transformers Fall of Cybertron\TransGame" ..\assets
```

Left out: the blade's edge's distortion of the scene behind it, and the effects the
swing's clip starts on the robot (`DashPulse_1_FX` on his boosters, the
`Trails_Bumblebee_FX` streaks on his wings, elbows and knees).

### The class abilities

Every class's other abilities work as the game runs them (`src\abilities.rs`, with
`src\cloak.rs`, `src\whirlwind.rs`, `src\healbeam.rs`, `src\flakshield.rs`,
`src\hoverslam.rs` and `src\battlesentry.rs`, from `TransGame.xxx`'s `TnAbility`
classes, `mp_shared.xxx`'s and `MP_Array_BASE_m.xxx`'s `TransContent` ones and the
game's `AbilityResourceDatas`): the Infiltrator's **Cloak**, the Destroyer's
**Diffraction Barrier** and **Hover Slam**, the Titan's **Whirlwind** and **Flak
Shield**, and the Scientist's **Heal Beam** and **Battle Sentry**. A build's ability
is the one its character plays with, and its ability upgrade works as the game's does.

- **What they share.** Each has a meter, full as Jazz spawns and each time he takes
  up his character. Press **Q** (in robot form) to start the ability: it needs enough
  in the meter, takes its starting share and drains while the ability goes on
  (running out ends it), then refills. Pressing Q again ends it early (the Heal Beam
  uses its presses itself), but not within a second of the last press that did
  something (the Hover's quarter of a second). Going down or dying ends what is going.
  The HUD (bottom right) shows its icon, the key and the meter round it, small and
  blue while it is in use or short of a use, with the game's ready sound as it comes
  ready, and the cancel icon while the Cloak or the Barrier is going. The Intercooler
  support item refills it faster and drains it slower.
- **Cloak** (a meter of 14: 10 to start, 2 taken). Jazz plays his gesture
  (`Nav_ToStealth`), its loop plays, the game's lightning (`Cloaking_FX`) crackles
  round him (at his robot's size, 0.6, as the game draws it on his robot mesh), and
  the cloak goes on as the game's `TnCloakMaterialOverrider` puts it
  on, drawn as the game's two cloak materials draw it (`assets\shaders\paint.wgsl`):
  for 1.5 s its overlay (`TR_Cloak_Overlay_MATINST`) is drawn over him, a ragged
  front spreading out from his right elbow to his size's reach, glowing blue where it
  passes, his body black with an orange rim behind it, then burning out to nothing;
  halfway through, he goes into the cloak itself (`TR_Cloak_MATINST`), which shows
  nothing of him but the picture behind him shifted by his shape (as the game's
  distortion pass shifts it), and casts no shadow. As in the game's Escalation, it
  gives no warning as its meter runs low: the waves running out over him from his
  elbow in the last quarter of the meter, and their warning sound
  (`CLOAK_AUTOBOT_POWER_OUT`), are the campaign's only
  (`TnCloakMaterialOverrider.IsCampaign`; `src/cloak.rs` keeps them, off, as
  `CAMPAIGN`). It drains 1 a second as he walks and 3 as he sprints,
  dodges or boosts (12 s and 4 s from full), and goes on into the vehicle. However it
  ends he is seen again at once, with the lightning again. Shooting, a blow, a grenade
  or damage gives him away: the cloak ends and he can't cloak again for 1.5 s.
  **Specter** halves the drain, **Cloak Intercooler** doubles the refill, **Shadow
  Stalker** gives him away only 1.25 s after his own attacks.
- **Diffraction Barrier** (20, drained 2 a second: 10 s; refills in 20 s). The support
  item's wall (above), raised with the same gesture. Q again lets it down; using the
  support item's barrier lets the class's one down, and the other way round.
  **Fortify** halves the drain.
- **Whirlwind** (20: 10 taken, 2.1 a second: 4.76 s). On the ground only: his upper
  body spins in the Titan's own clip (`Titan_ROBO_ANIM`'s `Transform_Whirlwind_ROBO`,
  1.67 s: he winds up, then goes round and round from 1.36 s, a turn every 0.31 s, as
  the clip's `LoopOffsetTime` has it) while his legs walk and run as they would. As
  it starts his gun's Ion Blade slides out (the clip's `TnAnimNotify_WeaponMelee`),
  and it slides back in once he stops. Each turn of the spin he sweeps out the game's
  two whirlwind trails, each played 3 m out along its socket and carried round with
  it: a white-hot burst that bends the picture behind it (`Weapon_Trail_Melee_FX`,
  on his `WeaponTrailSocket`) and a smear of distortion (`Sword_Trail_Whirlwind_FX`,
  on his `WeaponSocket_Melee`), with the wind-up's `WHIRLWIND_START` and the whooshes (`WHIRLWIND_WHOOSH_SINGLE`,
  twice in the wind-up and once a turn). The camera goes out from 8.75 to 11.75 m and
  widens to 90 degrees while he spins (the class camera's `TnPCS_Whirlwind`). Two
  sweeps in the wind-up (0.9 and 1.12 s, within 5.5 m of him) and one each turn (6.5
  m) strike every enemy in reach for 60; with a full bar 14 land. He can't shoot or
  melee while he spins. **P.o.K.E. Alpha** strikes 1.25 times as hard, **Energon
  Absorbers** takes 0.85 of the damage he takes, **Flurry of Blows** moves him 1.15
  times as fast, all while he spins. The other players see and hear his spin too.
- **Heal Beam** (15, drained 1 a second while linked; refills at 2). Q links the
  beam to the teammate under the crosshair (within 10 degrees of it, in sight, within
  45 m), healing him 75 a second (his health, then his shield), with the beam reaching
  to Jazz's free hand, the healing glow on him and the beam's loops. With nobody there it
  fails (its sound). Q again switches to another teammate under the crosshair, or ends
  it. The link breaks beyond 60 m, out of sight for 1.25 s, with him dead or down, or
  with the meter empty. While the one healed is short of full, the HUD's icon fills
  (0.33 a second); full, it rewards Jazz: **Energon Recharge** heals him 100,
  **Battle Medic** makes his blows 1.15 times as strong for 10 s, otherwise the beacon
  effect. **Empowered Beam** halves the drain. (Playing alone, there's nobody to heal.)
- **Flak Shield** (4.5, drained 0.25 a second while up: 18 s). The game's shield
  unfolds on his forearm (`ShieldGrimlock_Equip`) and he moves in the shield stance
  (the game's `Nav_Shield_*` clips) at half speed unless he sprints. Blows from in
  front of him are stopped, each point taking 0.015 off the meter (300 from full). Q,
  his shooting or a melee, or changing form lowers it; the meter running out **breaks**
  it (the break, its sound and effect) and he can't raise it again for 5 s. **Basher**
  lets him move at 0.75 speed, **Brute Force** halves what blows take off the meter,
  **Blastproof** heals him a tenth of each blow stopped.
- **Hover Slam** (20: 10 taken, 2 a second while hovering: 5 s). He jumps 12.5 m and
  **hovers** at the top: no gravity, moving at 6 m/s, shooting as he likes, his blows 1.5
  times as strong and those on him half as, the boosters burning and the hover's loop
  playing, with the red rings on the ground where a slam would land. Q again **slams**:
  he dives forward and down, and landing strikes every enemy within 5 m for 150 with the
  game's ground smash (`HighLeap_GroundSmash_FX`) and a hard shake of his view, then
  recovers as from a hard landing. Jump, a blow or the meter running out drops him
  instead. **Divebomb** widens the slam to 6.25 m, **Bug Out** doubles his hover speed,
  **Aerial Camper** stops the drain while he hovers in place.
- **Battle Sentry** (30: 5 taken, 0.5 a second while it lives: 50 s; refills at 1.5).
  The game's sentry drone comes out 4 m ahead of him and 3 m up, in his colours, and
  hovers there. After 2 s folded it opens and hunts the nearest enemy in sight within
  60 m, turning to it at 270 degrees a second with its laser sight on, and once aimed it
  fires a round every 0.15 s (8 each, its tracers and impacts, the spread growing as it
  fires). Q again destroys it, as does the meter running out (its blast); it goes with
  him. **Power Core** makes its rounds 1.4 times as strong, **Resource Recycler** stops
  the drain, and **Rust Sentry** brings out the slow sentry (40 m, with a beam in place
  of the gun that slows, and does no damage).

Online, each player's PC runs his ability, and the others see it: his cloak, barrier,
spin, beam, shield, hover and sentry, with their sounds, and the damage his sweeps,
slams and sentry's rounds deal reaches their copies of the enemies too (the network
protocol is now version 29; version 38 sends his meter with it, so they see his
cloak run low). A teammate's Heal Beam heals you on your own PC.

The Flak Shield's and the sentries' models (`models\abilities\`) come from
`tools\build_abilities.py`, from umodel's export of them (and of their texture sets'
pictures, which the script's notes list):

```
cd Documents\foc_esc\tools
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=C:\temp\abilities mp_shared.xxx -obj=AP_AttackSentry_SKEL -obj=AP_AttackSentrySlow_SKEL -obj=WEP_ShieldLeader_SKEL
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=C:\temp\abilities mp_shared.xxx -obj=AP_AttackSentry_ANIM -obj=WEP_ShieldLeader_ANM
python build_abilities.py C:\temp\abilities "C:\...\TransGame\CookedPC" ..\assets
```

the abilities' clips (`models\robot_extras.glb`) from `tools\build_robot_extras.py`;
their effects from `tools\build_weapon_fx.py`; their HUD icons from `build_hud.py`
and their sounds from `build_sounds.py`. The cloak's textures (`textures\cloak\`: its
overlay's noise mask and gradients, its distortion's clouds and normal maps) come from
`tools\build_cloak.py`, which also checks that the game's two cloak materials still
have the values `paint.wgsl` draws them with:

```
python build_cloak.py "C:\...\TransGame\CookedPC" ..\assets
```

Not in: the cloak on the Sharpshooters (their cloak works, but they are drawn as
they are). Not in: the outline the game draws round the one a Heal Beam heals, the hover's own clips
(they are the truck's skeleton's: the Infiltrator skeleton's hover pose stands in),
and what Ensnare and One-Way Barrier do (the enemies here don't move or look).
Since the enemies don't fight yet, the Cloak's hiding, the Flak Shield's blocking and
the Barrier's cover have nothing to stop.

### The Riot Cannon

The Riot Cannon is the game's `TnWeaponBazooka` (the "Bazooka" and, in its sounds,
the "Fusion Cannon" inside), played with the data Escalation gives it
(`WEP_Bazooka_p`'s `Bazooka_SV_WEPDATA`, from `all_shared.xxx`) and the numbers of
`TransGame.xxx`. Unlike the other guns it fires rounds that fly (`src\riot.rs`):

- **Firing.** Automatic: a round every 0.85 s while the trigger is held (its
  `FireInterval`), with a steady spread of 0.04, half that in fine aim, and the
  game's cannon reticle. Every shot kicks the view up 1.5 degrees from the hip, the
  field of view a degree wider for a moment, and 0.75 degrees in fine aim, each easing
  back over 0.35 s after a tenth of a second (`CameraRecoils`,
  `FineAimCameraRecoils`), and jolts it the hardest of the guns (`CameraShakes`:
  intensity 0.65 with a roll, over 0.4 s). The round leaves its barrel's
  `MuzzleFlash` socket towards what the crosshair is on, an enemy or a wall
  (`ProjectileFire` turns it to where the shot's trace struck). 4-round clips, 24
  in reserve, 2.5 s reload.
- **The round** is a `TnProjectileBazooka`, whose data in Escalation is its
  `Bazooka_SP_PROJDATA` (the game's `GetProjectileData` gives a projectile with no
  survival data its own): it flies dead straight (no gravity) at 180 m/s
  (`InitialSpeed` and `MaxSpeed` 18000) for at most 4 s (the projectiles'
  `LifeSpan`, at the end of which it goes off where it is), and goes off on the
  first thing in its way: the level, an arena pillar, an enemy or another player. It
  passes through only the player who fired it (see *What changed for every round*,
  under *The Destroyer's weapons*).
- **Going off** (`Projectile.HurtRadius`): 155 damage (`Damage`) to every enemy
  within 6.5 m (`DamageRadius`, reaching anything it overlaps), as much at the edge
  as at the heart (its `RadiusDamageScale` has no curve) and through walls
  (`IgnoreHurtRadiusTrace`). It doesn't hurt Jazz or the other players: Escalation's
  `SelfDamageMultiplier` is 0 (`[TransContent.TnSurvivalGame]`, over `TnGame`'s 0.25
  for the other games, applied in `TnPawn.AdjustDamage`).
- **Online**, each round fired goes to the other players' PCs (`net.rs`'s `Round`;
  the network protocol went to version 9 for it, so every PC needs the same version), where it
  flies and goes off the same way, striking their enemies as their copies of shots
  do.

Its effects are the game's own particle systems, which `src\levelfx.rs` runs as it
runs the EMP grenade's (see *The EMP grenade*) and `tools\build_weapon_fx.py` reads
with them:

- **`RiotBazooka_MuzzleFlash_FX`** (`FX_GrenadeLauncher_p`, its weapon mesh's
  `TnWeaponEffectPlayMuzzleFX`) on the gun's `MuzzleFlash` socket, following the
  barrel while it plays: the muzzle cap mesh (`MuzzleFlash_STATMuzzleFlash_Cap_STAT`)
  flaring blue and spinning, a ring of lightning and rings blowing out along the
  barrel, a quick glowing cloud and purple smoke rolling out of the barrel. Its light
  is the weapon's `TnWeaponEffectLight`, violet.
- **`Tracer_Bazooka_FX`** (`FX_HomingRocket_p`, the round's `FlightEffect`) on the
  round: a blazing blue-white ball of glows and bursts, a ribbon trailing from it, and
  purple smoke left behind it, a puff every 50 cm of its flight. The round carries
  its own light (its `PointLightComponent`: colour 100, 65, 109, radius 8 m) and its
  crackling flight loop (`TRAIL_LP`). When it goes off, what it has given off lives
  out its life (its `bWaitForFlightEffect`).
- **`RiotCannon_ImpactExp_FX`** (`FX_OS_Weapons_p`, the round's `ExplosionEffect`),
  turned to the struck surface (its X along the surface's normal) and drawn at 650 /
  750 of its size, as `HmProjectile.SpawnExplosionEffect` does (`DamageRadius` over
  `ExplosionEffectRadius`), with the blast sound (`EXPL_IMPT_WORLD`): a white flash
  with purple rays, a glow brightening the surface round it, a burst of fire and
  smoke, fire lingering red, a hundred white sparks and embers.

For these, `levelfx.rs` now also runs three more of the game's particle modules,
which `tools\ue3\particles.py` reads from High Moon's compiled form (whose spawning
modules have a list of their own): particles spawned by the distance the emitter
moves (`SpawnPerUnit`: the smoke trail), particles born where another emitter's
particles are (`LocationEmitter`: the ribbon on the ball's glows), and mesh particles
that keep spinning (`MeshRotationRate`: the muzzle cap); an effect can also be drawn
at a scale (the explosion's). And, as the game does:

- a spawned effect plays at the **level of detail** its distance from the camera
  calls for as it starts (each system's `LODDistances`): the explosion, for one,
  has a far version from 10 m on that throws its smoke out all at once, and the
  round's from 20 m. `build_weapon_fx.py` writes each system's other levels where
  they differ, as systems of their own (`<system>@<level>`, with `lod` lines);
- a moving effect is drawn where it is that frame (it was a frame behind: the round,
  3 m back at 60 frames a second), and a new one from the frame it starts;
- a decal emitter's mark (High Moon's own kind: the squibs' glowing bullet holes) is
  laid once, on the surface a trace along the emitter's direction finds from just
  in front of the impact, within the mark's reach, and cut to the level's surfaces
  round there that face it (the map's collision, which is its meshes), so it wraps
  over a doorframe and the wall beside it rather than hanging past an edge in the
  air; with nothing in reach there is none. Each impact lays one mark, as the game's
  data has it (the decal emitters' cooked `PeakActiveParticles`): their spawn rates
  would lay 4 to 20, one on another;
- **the blasts' scorch marks** are left out (`build_weapon_fx.py`'s
  `UNSEEN_DECALS`): `DeathDecal_MAT`, a black burn with glowing specks, which the
  explosions' decal emitters lay 8 to 40 m across (the tank shell's 12 to 13.5 m,
  the rockets' 8 to 18 m, Downfall's sentries' 30 to 40 m) and the deaths' gibs 5 to
  15 m. Drawn as the data has them, each left the floor round a blast near-black
  for up to half a minute, and a few close tank shots blacked out most of the
  view; the game's videos of a tank shell striking the floor ahead, and of
  Downfall's sentries' rockets and the soldiers they blow up, show the floor
  unmarked after. (The Path Blaster's small scorch on the walls, 1.25-1.6 m, built
  by hand in `src\fx.rs`, stays.)
- additive materials are added as much as their opacity says, as Unreal adds them:
  the Riot Cannon's clouds keep their shapes in their textures' alpha, its muzzle
  cap shows only side on (it is a star of planes along the barrel), and the maps'
  glow spheres fade where they meet walls and as you come close. An opacity over 1
  counts in full, and so does a particle's starting alpha over 1 (its Color
  module's): the game's compiled shaders multiply by the opacity unclamped
  (`LightningBeamTrail_MAT`'s, read from its shader cache), and High Moon's compiled
  Color module keeps no clamp. The Electric Fences' lightning (starting alpha 6)
  came out a third as bright as in the game's videos while both were held to 1; of
  the other effects, about thirty emitters change (sparks, muzzle glows and flashes
  whose materials use the alpha that way), most of them a little;
- **the effects' textures are filtered anisotropically** (16 samples along a
  pixel's long side), as the game filters its default texture group, which nearly
  all of them are in. It shows on beams and trails, their textures drawn out along
  them: the Electric Fences' lightning texture (512 texels along 31 m, 256 across
  2 m) was read from a mip four times smaller, its thin bolts smeared into a dim
  haze;
- a material's dynamic parameters are 1 where its emitter has no module setting
  them (the Intercooler's lightning, the Riot Cannon's muzzle clouds). (`TFOC.exe`
  reads them from the start of each particle's own data there, its last position
  and its age; taken so, at the maps' world positions in the thousands, the muzzle
  smoke of the Riot Cannon and the heavies, `Clouds_Anim_MAT`, whose colour they
  mix, came out as screen-filling yellow and white smears in the test pictures,
  which no video of the game shows, where with 1s it is the even purple haze the
  videos show);
- of the emitters whose material and modules disagree about dynamic parameters,
  the game draws those whose material reads them where no module of theirs sets
  them, and never shows those with a `DynamicParameter` module whose material
  doesn't read them. Fall of Cybertron compiles a particle material's shaders with
  dynamic parameters only if the material reads them, and without them only if it
  doesn't, which was once taken to mean that neither kind has shaders in the game,
  and some 300 were left out of the weapons' effects. The first kind does show:
  the game picks the shaders by the material (`TFOC.exe`: the sprite, sub-UV and
  beam and trail emitters' render data set `bUsesDynamicParameter`, and pick their
  vertex factory, from the material's resource), and its videos show them - the
  Intercooler's buff, both of whose emitters are such, crackling with lightning and
  cyan streaks round the player, the EMP grenade's lightning on whoever it catches.
  Some 200 of these are back: among them the Riot Cannon's and the heavies' muzzle
  flashes' brightening bursts (`Mod_Glow2x2_MAT`) and clouds and the Riot round's
  flashes, the Path Blaster's, TechVolt's and saw blade's lightning, the glows
  under the vehicles' hover jets and in the jets' afterburners, and the explosions'
  metal bits and specks. The second kind, though their materials have shaders for
  plain particles, are nowhere in the game's videos: the boosters' cross beams
  (`Booster_Jazz_FX`'s `CrossBeamVert_Export` and `CrossBeamHorz_Export`,
  `Glow_Mod_Depth_MAT`: glows 10 to 15 m long through each booster, which no video
  of a boosting car shows) and the X-18 Scrapmaker's dark smoke (`Smoke_Dark`,
  `Clouds_Basic_MAT`: dark puffs growing to 5 m across in front of the muzzle,
  which its videos never show against the bright corridor). So `build_weapon_fx.py`
  still leaves those out (some 120: the cross beams in the boosters and the jets'
  afterburners, the heavy weapons' dark smoke, the Decepticon jets' contrails and
  some of the deaths' sparks among them), with the three fires the game has no
  shaders for at all, in the rockets' and the Pulsar Cannon's impacts
  (`Squib_HomRocket_01_FX`, `Squib_RocketLauncher_FX` and `Squib_Pulsar_Lrg_FX`:
  sub-image emitters whose material, `Fire_Lrg_MAT_INST`, has no shaders for
  sub-image sprites with dynamic parameters). Ignition's flame jets' videos show
  two of the second kind all the same: Grimlock's flamethrower's `FlameTip`, the
  white-hot tongue of fire shooting from the nozzle into the billows
  (`Fire_Anim_03_MAT`), and its `FlameBalls`, the bright core inside them
  (`Fire_Anim_05_MAT`), so `build_weapon_fx.py` keeps those two (its `shown`
  option); without them the jets' fire hung 10 m off the nozzle past a short blue
  flame. So do the Whirlwind's: the Titan's spin's white-hot trail on his hand
  (`Weapon_Trail_Melee_FX`'s `Trail_Dup`, `Faction_Whirlwind_MAT`), the glare and
  the smear round him as he spins, kept for the Whirlwind as
  `Weapon_Trail_Melee_Whirlwind_FX` (the enemy Titans' blows, which play the same
  effect, are left as they were). Its material samples a cube map
  (`ENV_Metals_CUBEMAP`) in a plain texture sample, which `tools\ue3\fxmat.py` now
  reads as a cube map's sample where the packages have no 2D texture of the cube
  map's name to stand in for it (the materials that have one sample it as before). `tools\ue3\shadercache.py` reads which shaders each material has from the
  shader caches cooked into the packages.

The cap mesh comes from umodel's export (with the EMP grenade's dome, above):

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=C:\temp\fxmesh all_shared.xxx Sphere_03_MESH
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=C:\temp\fxmesh all_shared.xxx MuzzleFlash_STATMuzzleFlash_Cap_STAT
python build_weapon_fx.py "C:\...\TransGame\CookedPC" C:\temp\fxmesh ..\assets
```

Its sounds come from `build_sounds.py` (`BL_WPN_GUN_FUSION_CANNON`, see *Sound
effects*), its icon and reticle from `build_hud.py`, and its mesh, textures and
animations from `build_assets.py` (above). Its second fire mode, the
Planet Buster upgrade's last round of each clip (`TnProjectileBazookaSpecial`: 140 m/s,
500 damage within 20 m, its ball of lightning `LightningSphere_Tracer_FX`, its blast
`EXP_PlanetBuster_FX` and its own sounds and shake), is in with the upgrades (above).
Left out: the struck surface's own scorch (the physical materials' decals) and the
explosion's scorch mark (see *the blasts' scorch marks*, under the Riot Cannon's
effects).

### The Orbital Pistol

The Orbital Pistol is the game's `TnWeaponTracerDart` (the "TracerDart" inside: the
Scientists' gun in multiplayer, built on the campaign's SysCorruptor), played with
its multiplayer data (`WEP_TracerDart_p`'s `TracerDart_MP_WEPDATA`), which, with the
gun's class, mesh and effects, is cooked into the multiplayer maps
(`MP_Array_BASE_m.xxx`); the marks' numbers are `TransGame.xxx`'s (and
`TransWeapons.ini`'s). Its shots are instant hits like the other guns'; what it does
beyond them is in `src\orbital.rs`:

- **Firing.** Automatic: a shot every 0.85 s while the trigger is held (its
  `FireInterval`), 100 damage (`InstantHitDamage`) out to 300 m (`Range`). Its
  `RangeDamageScale` keeps it at full damage out to 20,000 units (200 m: two
  `DST_Constant` scales, to 10,000 and 20,000) and halves it beyond (a
  `DST_MaxRange` of 0.5), the distance measured as the game measures it
  (`TnWeapon.ModifyDamage`): from the middle of Jazz to the middle of the enemy,
  not from the muzzle to where the round struck as the demo's other guns do. Its
  spread starts at 0.02 and opens by 0.25 a shot (`ModifierChangePerShot`), which
  takes it straight to its widest, 0.15, settling back over a second (`Cooldown`),
  so a shot after a short rest is the sharp one; half that in fine aim. Its
  `CameraRecoils` are zero (it doesn't kick the view) and it has no
  `FineAimCameraRecoils`; its `CameraShakes` jolt the view a little (intensity 0.05
  with a roll of 0.15, over 0.15 s). 6-round clips, 24 in reserve, 1.25 s reload
  (`WeaponReloadAnimTime`); its low-ammo shot is the same sound (2 rounds left). Its
  reticle is the rifle's: the HUD's `updateCrosshair` has no case for
  `TnWeaponTracerDart`. Its HUD icon is the SysCorruptor's (the weapon icon's
  `checkLinkageId` turns `ItemWireframeTracerDart` into it).
- **Marks** (`TnWeaponTracerDart.ProcessInstantHit`): a shot that strikes an enemy
  does its damage first, then marks it with `TnBuffHardLocked`; struck again while
  marked, the mark is taken off and the next put on (`TnBuffHardLocked2`, then
  `TnBuffHardLocked3`, which further hits put on again), each for its full 10 s (the
  marks' multiplayer `BuffTime`). A marked enemy takes 1.1, 1.2 or 1.3 times the
  damage of everything that hurts it (the marks' `FloatModifier`; the game's
  description says it "decreases their armor"): the Orbital Pistol's own next shots,
  the other guns' shots, the Riot Cannon's blasts and the other players' shots.
  The mark shows on it:
  `OrbitalPistol_Marker1_FX`, `Marker2_FX` or `Marker3_FX` (the marks' `ApplyFx`),
  arrows closing in on it from either side, over and over, in an orange so bright it
  is nearly yellow (colour 1, 0.5, 0 at twice the strength): one a side for the
  first mark, two for the second and three for the third (`TargetArrow_MAT`,
  `_2_MAT` and `_3_MAT` repeat the arrow once, twice and three times), turned to
  face the camera (`TnBuffHardLockedBase.ClientTickBuff`), and gone with the mark.
  A mark is the last Jazz's to hit it, and it ends early if he dies
  (`TnBuffHardLockedBase.GetBuffState`: its `Instigator` dying or dead).
- **Revealing** (`TnBuffRevealPositionToEnemy`, which every hit puts on as well, a
  buff of its own): for 8 s after each hit, even if the marker dies in that time
  (then with no mark's icon), the enemy shows on the HUD of every player on the
  marker's team, wherever it is, walls or no
  (`TnObjectiveMarkerTypeTransformerVersus.ShouldDisplayEnemyMarker`): the HUD
  movie's `mc_markerPawn` for an enemy, in the enemy colour (`HudData`'s
  `EnemyColor`, #FF8787), with no name or health bar (the game shows an enemy's
  health only to a player with a fine-aim marker upgrade), so the marker's arrow
  (its `icon_mc`, a chevron pointing down) shows, unfolding as it appears, and over
  it the icon of the enemy's mark (`buffs_mc`, as its `updateBuffs` lays it out:
  `BuffIconHardLocked`, `BuffIconHardLocked2` or `BuffIconHardLocked3` at 65%, in the
  marker's colour). Off the screen it hides, as the other players' markers do.
- **Online**, every PC marks its own enemies as each shot reaches it (`fire` and
  `remote_shot`), so the marks, their arrows and the reveal show to all the
  players. The network protocol is now version 10, so every PC needs this version.

Its effects are the game's own particle systems (`tools\build_weapon_fx.py`),
which `src\levelfx.rs` runs:

- **`TracerDartMuzzle_FX`** (its `TnWeaponEffectPlayMuzzleFX`) on the gun's
  `MuzzleFlash` socket, following the barrel while it plays (as the Riot Cannon's
  does): glows at the barrel, and lightning bolts in blue and purple crackling
  round it (`Lightning_4x4_MAT`: see *The Riot Cannon*'s note on the game's
  shaders). Its light is
  the weapon's `TnWeaponEffectLight`: a pale cyan (117, 255, 229) at the default
  brightness.
- **`TracerDart_TracerBeam_FX`** (its `TnWeaponEffectPlayTracerFX`), started at the
  muzzle and reaching to where the shot struck (`SetTracerDistance`): two beams, a
  thin core (`Tank_Tracer_MAT`, a blinding yellow-white: colour 90, 85, 0.1) and a
  wide band of panning lightning (`Lightning_Panning03_UVset1_Mesh_MAT`), there at
  once (their `Speed` is 0) and gone in a fifth of a second (their `Lifetime`). Its
  third emitter, a burst of bolts, is drawn too, at its size in the game: a
  twentieth of a millimetre, so it never shows.
- **`TracerDart_Exp_FX`** (its `TnWeaponEffectPlayImpactFX`'s
  `DefaultParticleSystem`) where the shot struck, along the surface's normal: a
  burst of fire and smoke, a splash and a brightening flash.
- **`TracerDart_Idle_FX`** (a `TnWeaponEffectPlayFXOnSocket` on its `ShellEject`
  socket, from `ActiveStart` to `ActiveStop`): wisps of dark steam drifting from
  under the gun while it is ready in Jazz's hand (`TnWeapon`'s `Active` state:
  firing, reloading, putting it away, switching hands and changing form end it, and
  what is out lives out its life), carried and turned with the socket.
- It steams at Jazz's elbow as it reloads (`ReloadSteam_Med02_FX` on its `Pickup`
  socket, as the Path Blaster's), and throws no shells.

For the tracer, `levelfx.rs` now draws **beams** (`ParticleModuleTypeDataBeam2`,
which `tools\ue3\particles.py` reads with the emitter's `BeamSource` and
`BeamTarget` modules): each beam particle a ribbon from its source to its target,
facing the camera, as wide as twice its size times its taper (`TaperFactor` times
`TaperScale`, both along the beam), the texture running along it (`TextureTile`), at most
`MaxBeamCount` at a time; a tracer's beams end where `FxSystems::spawn_tracer` says
(the game's `SetTracerDistance`). Beams that grow out to their target over time, or
that are shaped by modules the demo doesn't run (the EMP grenade's blast has one
connected to the bones of what it hits, `BeamConnectBones`), are left out, so the
EMP grenade's effects are unchanged.

Its mesh (`RB_SysCorruptorR_SKEL`, `RB_SysCorruptorL_SKEL`) and animations
(`Weapon_TracerDart_ANIM`: the campaign gun's `ADD_Weapon_SysCorruptor_*` clips) are in
`all_shared.xxx`; its textures' full-size pixels are in `WEP_TracerDart_p.xxx`
(`WEP_SysCorrupter_TEXSET`). Export them into the same `all_shared` folder as the
Riot Cannon's:

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_SysCorruptorR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_SysCorruptorL_SKEL SkeletalMesh
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_TracerDart_ANIM AnimSet
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport WEP_TracerDart_p.xxx
```

(and copy the two `WEP_SysCorrupter_TEXSET_*.png` from
`UmodelExport\WEP_TracerDart_p\Texture2D` into `UmodelExport\all_shared\Texture2D`).
Its sounds come from `build_sounds.py` (`BL_WPN_GUN_TRACER_DART`, see *Sound
effects*), its icon, the marks' icons and the marker's arrow from `build_hud.py`
(see *The HUD*), and its effects from `build_weapon_fx.py`. Its upgrades are in
(Sonic Blast's faster fire, Energon Stinger's stronger marks, Orbital Trace Hack's
longer ones: see the weapon upgrades above), and its marks count for experience
(Visionary, and an assist when another kills what you marked). Left out: the decal it leaves where it strikes a wall (its `DefaultDecal`, as
the Photon Burst Rifle's, which has the same one, is left out: the game picks a
surface's own from its physical material), the off-screen arrow pointing to a
revealed enemy, and the challenges the game counts for marks.

The enemies have their types' own health (a soldier 150), so a marked one takes
the marks' extra from every hit after the first: a soldier struck once by the
pistol (100) and then by the rifle takes 19.8 damage a round instead of 18.

### The Titan's weapons

The Titan's four weapons are the game's, played with the data a match of
Escalation gives them: their Escalation data (`SurvivalData`) where the game has
some, the X18 Scrapmaker's `HeavyMG_SV_WEPDATA` and the E.D.K. TechVolt's
`ChainBeam_SV_WEPDATA`, and otherwise their multiplayer data (`MultiplayerData`), the
Energon Harvester's `DrainGun_MP_WEPDATA` and the Corrosive Slime Cannon's
`AoeSlow_WEPDATA` (the game gives a weapon its Escalation data, then its
multiplayer data, then its campaign data: `TnWeapon.GivenTo`; the campaign's are
other versions of the guns, a Slime Cannon whose slime eats away at what it
lands on, say). Their weapon data, classes, meshes and effects are cooked into
`all_shared.xxx` (their `WEP_*_p` packages, and their scripts in `TransContent`), the
classes they build on in `TransGame.xxx`. Each is an entry of `WEAPONS` in
`src\main.rs` like the guns before them; what they do beyond those is in
`src\arsenal.rs` (with the Slime Cannon's round in `src\riot.rs`):

- **The X18 Scrapmaker** (`TnWeaponHeavyMG`). Its fire interval is an
  `HmValueLerper` (its `FireIntervals`: 0.35 s to 0.065 s over a `RampTime` of 1 s)
  that moves on while it is in its firing state and back at the same rate out of
  it (`TnWeapon.UpdateFireInterval`), and its refire timer is set afresh at each
  round's interval (`HmWeapon.WeaponFiring.RefireCheckTimer`). While the interval
  is over its `TnWeaponDataExtensionHeavyMG`'s `MinFireIntervalToStartFiring` (0.15
  s), `TnWeaponHeavyMG.FireAmmunition` fires nothing: it warms up instead (its
  `StartCharging` event: the wind-up cue, `BL_WPN_GUN_PULSE_RIFLE.SHOOT_WINDUP`,
  which plays on to its end if you let go, and the gun's `WarmUp` clip), so with
  the trigger held the first round leaves at the fourth refire check, about 0.78 s
  after the pull. That first round is its `StartFire`: from then on its loop
  (`SHOOT_LP`) is held under each shot's sound and its firing clip loops, until the
  firing stops (`StopFire`: its tail, `SHOOT_TAIL`, and its `CoolDown` clip; let go
  while it is still warming up, `StopCharging` plays the cool-down alone). Its
  rounds leave its two `MuzzleFlash` sockets in turn. Online, the other players'
  PCs are told when a Jazz's gun is warming up (a change to the network protocol:
  every PC needs this version).
- **Moving while firing.** The game's pawns multiply their speed by named factors
  (`TnPawn`'s ground speed modifiers): a weapon's `GroundSpeedMultiplier` while it
  is in hand and its `FiringGroundSpeedMultiplier` while it is in its firing state
  (`TnWeapon.OnBeginStateWeaponFiring` to `OnEndStateWeaponFiring`). The
  Scrapmaker's and the TechVolt's are 0.75 while firing; the demo now applies
  these to every gun, so the Subsonic Repeater (1.05 in hand) and the Scatter
  Blaster (0.85 while firing) have theirs too.
- **The heavy-gun stance.** The Scrapmaker's `BaseStance` and `FineAimStance` are
  `STANCE_HeavyGunArmShootFromHip`, for which the robot's anim tree
  (`TR_Shared_ANIMTREE_p.Robot_ANIMTREE`, its `TnStanceAnimationPickers`) plays
  `Nav_Idle_HeavyWep_Shoot` standing, `Nav_StrafeJog_HeavyWep_Shoot_*` and
  `Nav_StrafeWalk_HeavyWep_Shoot_*` moving (the ten strafe directions each, blended
  from the walk at 4.5 m/s and slower into the jog at 12 m/s and faster, its
  `WalkJogBlend`) and `Nav_Shoot_HeavyWep_IdlePivot90_L` / `_R` and `IdlePivot180_L` /
  `_R` turning on the spot (the same turns as the shooting stance's). They are in
  Jazz's `Shared_SCOUT_ROBO_ANIM` (`MP_Array_BASE_m.xxx`), and the Destroyer's and the
  Titan's characters have their own, from `Shared_Truck_ROBO_ANIM` (`mp_shared.xxx`).
  Each gun now takes its own time to come out (its `EquipTime`: 0.2 s for the
  Scrapmaker and the TechVolt, 0.75 s for the Harvester and the Slime Cannon; the
  first eight keep the demo's).
- **The Energon Harvester** (`TnWeaponDrainGun`): bursts like the Photon Burst
  Rifle's (`ShotsPerBurst` 2, `FireInterval` 0.2 s, `BurstDelay` 0.6 s), each shot its
  `NumShotsToFire` of 9 pellets laid out by its `SpreadPattern` as the Scatter
  Blaster's are. For every pellet that damages an enemy (`ProcessInstantHit`), Jazz
  gets its `TnWeaponDataExtensionDrainGun`'s `HealthPerHit` back: 15 in this data
  (the campaign's is 7). Its shot sound is a `TnWeaponEffectLowAmmoSound` on
  `FireShot`, so each shot of a burst has its own (the Photon Burst Rifle's plays
  once a burst); its heal effect plays on `HitEnemy` (and `Kill`).
- **The E.D.K. TechVolt** (`TnWeaponChainBeam`, a `TnWeaponBeam`): a round every
  0.07 s while the trigger is held, each an instant hit doing its
  `DamagePerSecond` (120) times the interval. Its beam is a looping tracer
  (`TnWeaponEffectPlayTracerFX` with `Looping`: `TechvoltEmitter_MuzzleFlashAndTracer_FX`
  held from `StartFire` to `StopFire`, its end moved to where each round strikes).
  `TnWeaponChainBeam.UpdateChainBeam` (its `TnWeaponDataExtensionChainBeam`) finds
  every enemy within `MaxChainLength` (14 m) of the beam's line, inside its
  `BeamConeAngle` (55 degrees) from the start, that can see the nearest point of the
  line (`TraceSight`), leaving out the one the beam strikes, and draws its
  `ChainTemplate` (`TechvoltEmitter_ExtensionTracer_FX`) from it to that point; each
  takes `ChainDamagePerSecond` (80) times the time (the demo does it with each
  round). Its light is a `TnWeaponEffectLight` held from `StartFire` to `StopFire`
  (colour 96, 131, 245, `Brightness` 30, `Radius` 500, ramping in and out over half
  a second). Online, each round's end is sent with it, and every PC draws the
  beam and its chains. (The game's beam also sets off enemy projectiles near it,
  `TimeToStartDamagingProjectiles`; the demo's enemies don't fire yet, so there
  are none.)
- **The Corrosive Slime Cannon** (`TnWeaponAoESlow`): its rounds are
  `TnProjectileSlow`, whose only data is its multiplayer `Slow_MP_PROJDATA`: 7000
  units a second, a `GravityScale` of 1 (the world's `DefaultGravityZ`, -2940), a
  4 s life, and `HurtRadius` for 125 within 800 units (a `RadiusDamageScale` of a
  constant stretch to 400, a linear one to 800 and its `DST_MaxRange` of 0.5), with
  `SlimeCannon_Exp_FX` at `DamageRadius` / `ExplosionEffectRadius` (800 / 1000) of
  its size and the projectile mesh's light (`Radius` 800, `Brightness` 8, white). Its
  muzzle flash is the gun's `TnWeaponEffectPlayMuzzleFX`
  (`Corrosive_Slime_Cannon_MuzzleFlash_FX`). Its fine-aim button never fine aims:
  `TnPlayerController`'s fine aim tries the weapon's `TryAltFirePress` first, and
  this gun's turns its `TnSimulateProjectileExtension` on, which for the player's
  own pawn runs the round's flight every frame (`TnProjectile.RunPhysicsSimulation`:
  a place every `SimulationTimeStep`, 0.05 s, for up to `SimulationMaxTime`, 3 s,
  until it would strike something) and plays the projectile mesh's
  `TrajectoryPreviewEffect` along it; `levelfx.rs` replays the effect's particles
  along those places. The slime's damage type, `TnDamageTypeSlow`, slows enemies it
  hurts (`TnBuffAoeSlow`: robots to 0.75 of their speed, vehicles to 0.65, for 3 s)
  but never the player's own side (`ShouldApplyVictimBuffsToFriendlies` is off); the
  demo's enemies stand in place, so that is left out, as is the slimed look it
  gives them.

Their effects are the game's particle systems, which `tools\build_weapon_fx.py`
writes into `assets\fx\weapons_fx.txt` (with their meshes in `assets\models\fx` and
their textures in `assets\textures\levelfx`) and `src\levelfx.rs` runs, each on the
socket the game plays it on, kept there while the gun is in Jazz's hand (as the game
attaches them: `AttachToSocket`), including the Scrapmaker's barrel smoke, whose
socket rides a bone of its spinning barrels:

- **X18 Scrapmaker** (`X18_Scrapermaker_FX`, so spelt): `X18_Scrapmaker_MuzzleFlash_FX`
  on the barrel that fired (a black flash, a fiery blast, spark bolts, smoke,
  electricity and a beam of light; its light Brightness 15, colour 221, 104, 96),
  `Scrapmaker_Tracer_FX` with every round (a glowing trail and bolts of scrap racing
  out at 120 to 150 m/s), `X18_Scrapmaker_Squib_FX` where it strikes (spears of fire,
  embers and a blast), `X18_Scrapmaker_Idle_FX` on its `IdleFX` socket with every
  round (smoke off the barrels) and `Shell4_FX` on its `ShellEject` socket.
- **Energon Harvester** (`EMP_Shotgun_FX`): `EMPShotgun_MuzzleFlash_FX` with every
  shot (its light the default Brightness, 25, in 106, 181, 245), `EMPShotgun_Tracer_FX`
  once a burst (on `StartFire`: one tracer to the first pellet's impact),
  `EMPShotgun_Squib_FX` where each pellet strikes, `EMPShotgun_Idle_FX` on its muzzle
  while it is ready (`ActiveStart` to `ActiveStop`), `Harvester_PlayerHeal_FX` as it
  heals and `ReloadSteam_Harvester_FX` as it reloads.
- **E.D.K. TechVolt** (`Techvolt_Emitter_FX`): its beam, `TechvoltEmitter_Squib_FX`
  where each round strikes, the chains, `TechvoltEmitter_Idle_FX` on its muzzle while
  it is ready and `ReloadSteam_Tech_FX` as it reloads.
- **Corrosive Slime Cannon** (`Corrosive_Slime_Cannon_FX`): its round's
  `Corrosive_Slime_Cannon_MuzzleFlash_FX` (its light Brightness 3),
  `Corrosive_Slime_Cannon_Projectile_FX` flying and `SlimeCannon_Exp_FX` going off,
  `Slime_Trajectory_FX` along its path, `SlimeCannon_Idle_FX` on its `MeleeSocket`
  while it is ready, and `ReloadSteam_Slime_FX` as it reloads.

As the game's `HmParticleEffectManagerComponent.PlayEffect` does, an effect played on
a socket again while it is still playing either joins the one playing (its
`PlayCondition` `PEC_PlayAlways`: the shells, the barrel smoke, the steam) or starts
it over (`PEC_RestartIfAlreadyPlaying`, the default: the muzzle flashes, each
barrel's own, and the heal). For these, `tools\ue3\particles.py` now reads the list
of module indices that follows an emitter's spawn modules (the heal's and two rocket
squibs' emitters were left out before), and `tools\ue3\fxmat.py` translates the heal
crosses' material (`HealthRecharge_MAT`).

Their meshes (`RB_HeavyMGR_SKEL`, `RB_EmpShotgunR_SKEL`, `RB_RepairBeamR_SKEL` and
`RB_RustCannonR_SKEL`, and the `L` ones for the left forearm) and their animations
(`Weapon_HeavyMG_ANIM`, `WEP_EMPShotgun_ANIM2`, `Weapon_RepairBeam_ANIM` and
`Weapon_RustCannon_ANIM`) are in `all_shared.xxx`; each one's texture set's full-size
pixels are in its own package (`WEP_HeavyMG_p.xxx`, `WEP_EMPShotgun_p.xxx`,
`WEP_RepairBeam_p.xxx` and `WEP_AoeSlow_p.xxx`, the TechVolt's set being
`WEP_TechvoltEmitter_TEXSET` and the Slime Cannon's `WEP_RustCannon_TEXSET`). Export
them into the same `all_shared` folder as the Riot Cannon's (each gun's two meshes
and its animation set, then its package's pictures, copied across):

```
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_HeavyMGR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_HeavyMGL_SKEL SkeletalMesh
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_HeavyMG_ANIM AnimSet
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport WEP_HeavyMG_p.xxx
copy UmodelExport\WEP_HeavyMG_p\Texture2D\*.png UmodelExport\all_shared\Texture2D\
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_EmpShotgunR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_EmpShotgunL_SKEL SkeletalMesh
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx WEP_EMPShotgun_ANIM2 AnimSet
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport WEP_EMPShotgun_p.xxx
copy UmodelExport\WEP_EMPShotgun_p\Texture2D\*.png UmodelExport\all_shared\Texture2D\
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_RepairBeamR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_RepairBeamL_SKEL SkeletalMesh
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_RepairBeam_ANIM AnimSet
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport WEP_RepairBeam_p.xxx
copy UmodelExport\WEP_RepairBeam_p\Texture2D\*.png UmodelExport\all_shared\Texture2D\
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_RustCannonR_SKEL SkeletalMesh
umodel -game=trans -export -gltf -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx RB_RustCannonL_SKEL SkeletalMesh
umodel -game=trans -export -path="C:\...\TransGame\CookedPC" -out=UmodelExport all_shared.xxx Weapon_RustCannon_ANIM AnimSet
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=UmodelExport WEP_AoeSlow_p.xxx
copy UmodelExport\WEP_AoeSlow_p\Texture2D\*.png UmodelExport\all_shared\Texture2D\
```

`build_assets.py` adds them to `robot.glb` (the guns, and their clips after the
first eight's: `ANIM_NEW_GUNS` in `src\main.rs`), with the heavy-gun stance's clips,
and writes the guns as the Customize screen shows them (`assets\models\weapons`); the
classes' characters (`build_class.py`) get the stance's clips of their own and paint
the new guns' parts. Their sounds come from `build_sounds.py` (see *Sound effects*),
their icons and reticles from `build_hud.py` (see *The HUD*), and their effects from
`build_weapon_fx.py`.

Their upgrades are in (see the weapon upgrades above). Left out: the Scrapmaker's scorch on the wall
where its rounds strike (its impact's `DefaultDecal`, `MetalDecal_02`: as for the
Photon Burst Rifle, the game picks a surface's own) and its spent shell itself
(`Shell4_FX`'s casing mesh bounces off the floor in the game, `ParticleModuleCollision`,
which the demo's effects don't run, so it would fall through it: the shell's trail,
glow and steam play); the Slime Cannon's slowing and the slimed look; the TechVolt's
setting off enemy projectiles; and the weapon challenges the game counts for them (and
the experience they bring).

### The Scientist's Gear Shredder

The Gear Shredder is the game's saw-blade launcher (`TnWeaponSawblade`, the
"SawBlade" inside, `WEP_Sawblade_p` in `all_shared.xxx`), played with its Escalation
data, `SawBlade_SV_WEPDATA`. It fires `TnProjectileSawblade`, whose data for
Escalation is its `Data`, `SawBlade_SP_PROJDATA` (the game gives a projectile its
Escalation data, then its campaign data: `TnProjectile.GetProjectileData`; this one
has no `SurvivalData`, and its multiplayer data, `SawBlade_MP_PROJDATA`, is a slower,
weaker blade that homes before it bounces). Its entry in `WEAPONS` (`src\main.rs`) is
like the guns before it; what it does beyond them is in `src\arsenal.rs` (`Blades`,
`GEAR_SHREDDER`) and its blades are in `src\riot.rs` (`SAW_BLADE`, `Bounce`):

- **Charging.** `Active.StartFire` sends it into its `Charging` state, whose
  `BeginState` loads the first blade (`SetNumberOfBlades(1)`; `AddBlades` sends
  `StartCharging`, sets Jazz's speed to its `ChargingGroundSpeedMultiplier`, 0.85,
  attaches the blade's effect to the muzzle socket of the blade's fire mode and sends
  `AmmoLoaded`). `Charging.Tick` loads one more every `SecondsPerAdditionalSawBlade`
  (0.65 s) the trigger is held, up to three and no more than the clip holds.
  `Charging.StopFire`, letting go, fires (`SendToFiringState`), and after its
  `FireAmmunition` it drops the blades (`SetNumberOfBlades(0)`: `StopCharging`, the
  speed back). A pull while it isn't in `Active` does nothing (its own `StartFire`
  and `StopFire` are empty, and `Active.BeginState` clears any pending fire), so a
  pull during the half second after a shot (its `FireIntervals`) is lost. A reload
  drops the blades (`WeaponReloading.BeginState`), as does Jazz going down
  (`HolderDied`); it can reload while charging (`IsAllowedToReload`), and sprinting is
  off while it charges (`get_IsSprintDisabled`). (In the game the blades stay loaded
  through a weapon swap, as `Charging.TryPutDown` puts it down without dropping them,
  its charge's loop playing on; the demo drops them as it leaves Jazz's hand.)
- **The shot.** `GetNumShotsToFire` is the blades loaded and `GetShotCost` the
  `ShotCost` of the fire mode they make (1, 2 and 3 rounds); each round's fire mode
  is a step lower than the last (`SingleFireAction`: two, one, then none for three
  blades), its muzzle that mode's socket (`SawBlade_WEPMESH`'s `MuzzleSockets`:
  `MuzzleFlashCenter`, `MuzzleFlashRight`, `MuzzleFlashLeft`) and its aim the view
  turned by its `TnSawBladeExtension`'s `DesignPerBlade` spread (`AddSpread`: yaw 0, 910
  and -910, 5 degrees right and left). The shot's sound (`StartFire`) and its clip
  (`FireShot`) go by the fire mode (`ConvertBladesToFireMode` in `OnPlayFireEffects`):
  `SHOOT`, `SHOOT_CHRG_02_SAWS` or `SHOOT_CHRG_03_SAWS`, and `Fire_WEP` for one blade
  or `ChargedFire_WEP` for more, each with the arm's `Weapon_Recoil_HeavyLight_S` at
  1.667 times its speed.
- **The blades** (`TnProjectileSawblade` and its `TnSawBladeProjectileExtension`): 14000
  units a second in a straight line (no gravity), a cylinder 30 units round and 20
  high (`CollisionRadius`, `CollisionHeight`), for up to 20 s (its `LifeSpan`). Its
  `HitThing` bounces it off the level while it is younger than its `BounceDuration`,
  1.25 s (`Bounce`: its velocity mirrored in the surface at `BounceSpeedPercent`, 0.85,
  of its speed; the eleventh bounce is none, and it sticks in what it strikes next),
  and off the players' side at any age (`OnHitPawn`: `OnSameTeamAs`); it passes
  through the Jazz who fired it (`Projectile.Touch`). Its homing profiles in this
  data have only `AfterFirstBounce` (`MaxDistanceToTarget` 1000, `MaxDistanceFromPath`
  1500, `MaxAngleToTarget` 360, `HomingSpeedAfterFirstBounce` 100000): once it has
  bounced, `TickHoming` takes the enemy nearest its line within 10 m
  (`FindTargetInCone`, measuring from the enemy's body: `AI.CalculateRangeToPoint`),
  keeps it while it is ahead (`UpdateHomingTarget`), and turns the blade's velocity
  towards it by at most 1000 m/s each second (`HomeTowardsTarget`); a bounce turns it
  straight at a target still ahead (`UpdateHomingVelocity` with no limit) and drops
  one behind. Anything else it strikes, it sticks in (`Explode`): an enemy takes its
  `Damage`, 165 (it has no `DamageRadius`, so `Projectile.Explode` gives the struck
  enemy its damage alone), and the blade is set on it (`AttachToActor`: on its bone,
  with no offset: the demo's on its bone nearest where it struck, which it rides as
  the enemy moves and falls) until it is gone; the level holds it where it touched, 15 units out (`Projectile.HitWall`).
  Stuck, it shows its `StaticMesh`, `PROJ_SawBladeProjectile_STAT`, turned the way it
  was fired (it never turns as it flies: `bRotationFollowsVelocity` is off), for the
  10 s it is left (`DestroyProjectile`'s `LifeSpan`), its light out
  (`TnProjectile.PlayExplosion`) and its flight effect let go. Online, each blade is a
  `Round` of its own, flying, bouncing (off the players there but its shooter) and
  sticking on every PC.
- **The path fine aim shows** (`TnWeaponSawblade.UpdateTrajectorySimulation`): while
  Jazz fine aims with it ready or charging, a simulated blade for each blade loaded,
  one at least, from its fire mode's muzzle and aim, run for its `SimulationMaxTime`
  (1 s) a `SimulationTimeStep` (0.05 s) at a time, bouncing and homing as the blades
  do and ending where one would stick in an enemy, with its projectile mesh's
  `TrajectoryPreviewEffect` (`SawBlade_Trajectory_FX`) run along it, turned the way
  it is fired, its `Color` set at each step (`OnSimulationStepped`:
  `GetTrajectoryColor`, its alpha falling from 255 to 0 over the second): this
  data's extension's `FlyingOnMiss` (67, 130, 207) and `HomingOnMiss` (76, 57, 203)
  while the path shown before struck no enemy, `FlyingOnHit` (196, 103, 76) and
  `HomingOnHit` (196, 26, 34) once it did (stored blue, green, red, alpha, as UE3's
  `FColor` is), the trail's colour scale (3) making them a bright cyan and red: two
  lines fading out along the way, with their brackets.
- **Its effects** (`tools\build_weapon_fx.py`): the blade on each muzzle socket as it
  loads (`SawBlade_Weapon_FX`, the saw blade's mesh spinning, glowing brighter, with a
  crackle of electricity and its lightning), `SawBlade_Trail_FX` flying (the spinning blade, a mesh emitter,
  its spin rate following a curve over each copy's life,
  `ParticleModuleMeshRotationRateMultiplyLife`, which `tools\ue3\particles.py` and
  `src\levelfx.rs` now run; streaks off its edges and smoke), its light (`Radius` 350,
  `Brightness` 8, white), `Squib_PlasmaCannon_Med_FX` at a fifth of its size where it
  sticks (its `ExplosionEffectScale`), the stuck blade (written by the builder as a
  system of its own, `SawBlade_Stuck_FX`: one still mesh particle of the projectile's
  mesh and material), and steam as it reloads (`ReloadSteam_Med_FX` on its
  `ShellSocket` and `ReloadSteam_Med02_FX` on `ReserveAmmo`, a socket riding a bone of
  its moving parts). Its shot's light is `Brightness` 8 in 164, 232, 255. The
  squib's glows are coloured by the side that fired: the projectile mesh's
  `TnParticleSystemParameterTeamIndex` switches its effects' module channel to A for
  team 0, the Autobots, and to B for team 1, the Decepticons
  (`ParticleSystemComponent.SwitchModuleChannel`), and High Moon's
  `ParticleModuleSwitchableColorScaleOverLife` takes its A or B curves by it (warm on
  the Autobots' maps, blue on the Decepticons', as the factions' energon colours);
  the builder and `src\levelfx.rs` now run that module too (the jets' rocket flash and
  the A-4 Pulsar Cannon's round have it as well).
- **Its sounds** (`BL_WPN_GUN_SAW_BLADE`): its charge's loop held from the first blade
  to the shot (`CHARGE_LP`, fading over a tenth of a second), `CHARGE_MECH_01` as the
  first and second blades load and `CHARGE_MECH_02` for the third, the shot by its
  blades, its reload, and the blades' whirr in flight (`PROJECTILE_LP`), bounce
  (`PROJECTILE_BOUNCE`) and impact (`PROJECTILE_IMPT`); equipping, holstering,
  changing hands and fine aiming are the Neutron Assault Rifle's cues, as the game
  has them. It has nothing for pulling the trigger with no ammo (no `NoAmmoFire`
  effect).

It comes out in a fifth of a second (`EquipTime`), holds 6 rounds a clip and 12 in
reserve (2 s reload, `WeaponReloadAnimTime`), has no spread, and shows the game's
saw-blade reticle (`mc_crosshairSawblade`). Each shot widens the view by 1.5 degrees
(1 in fine aim), easing back over 0.7 s (0.65 s after a tenth of a second), with no
pitch (its `CameraRecoils`), and jolts it hard (its `CameraShakes`: intensity 0.5,
rolling 0.4 over 0.45 s, over half a second). Online, the other players' PCs are told
how many blades a Jazz has loaded (the network protocol is now version 18, so every
PC needs this version). Its mesh (`RB_SawbladeR_SKEL`, `RB_SawbladeL_SKEL`), its
animations (`Weapon_Sawblade_ANIM`: its charging loop, `Fire_WEP`, `ChargedFire_WEP`,
idle, reload, equip, holster and `MirrorReverse`) and its texture set
(`WEP_SawBlade_TEXSET`, full size in `WEP_Sawblade_p.xxx`) come in as the Titan's
guns' do (`build_assets.py`; export them the same way), and the projectile's mesh for
its effects with umodel's glTF export of `PROJ_SawBladeProjectile_STAT` (see
`build_weapon_fx.py`).

Its multiplayer upgrades are in (see the weapon upgrades above: Torque Ripper's faster
charge, `TnBuffSawbladeChargeTime`, Hyper Interlink and Tachyon Transmitter). Left out:
Escalation's homing-blade upgrade (`UpgradeHoming`, which multiplayer doesn't offer); its slicing of what is in front of the gun while
it charges (`SliceEnemies`: its `SliceDamagePerSecond` is 0 in this data, so it does
nothing anyway); cutting limbs off what a blade kills (`AttemptToDismember`) and the
push it gives (`MomentumTransfer`); the decal its blade leaves where it sticks (its
`TnDecalApplicatorProjectile`); and the weapon challenges the game counts for it (and
the experience they bring).

### The Destroyer's weapons

The Destroyer's three new guns are the game's, each played with the data Escalation
gives it: its `SurvivalData` where the game has one, else the data a match gives it
(`MultiplayerData`), and for each round the projectile's data the game picks
(`TnProjectile.GetProjectileData`: its `SurvivalData`, else its campaign `Data`).
Their entries in `WEAPONS` (`src\main.rs`) are like the guns before them; what each
does beyond them is in `src\arsenal.rs` (`THERMO_ROCKET`, `PULSAR_CANNON`,
`COMBUSTOR`: `LockOn`, `Detonate`, `Arc`) and their rounds are in `src\riot.rs`
(`THERMO_ROCKET`, `PULSAR_ROUND`, `COMBUSTOR_SHELL`, `COMBUSTOR_CLUSTER`).

**The Thermo Rocket Cannon** (`TnWeaponRocketLauncher`, the "HomingRocket" inside,
`WEP_RocketLauncher_p`; `RocketLauncher_SV_WEPDATA`, a `TnWeaponDataHoming`):

- **The lock** (`TnWeaponHoming`). Each frame its `Active` state picks the enemy under
  the crosshair (`TnTargetPickingTypeHomingLock`, its `TargetSnapRange` 168.52 m: the
  jets' picking, `src\jet.rs`), and kept there for its `LockOnTime`, 0.75 s, the gun is
  locked on (`StartLockOn`), the lock holding for `HoldLockOnTime`, 2 s, after the
  enemy leaves the crosshair. `CanLockOnToRobots` is off in this data, so it
  locks on only to the enemies not in robot form: the jets, the drones and the
  swarmers (`Enemy::robot`; the jets' rockets lock on to any). The lock is looked for only in `Active`
  (`UpdateLockOnWhileFiring` is off) with a round in the clip (`CanLockOn`), and the
  shot (`WeaponFiring.BeginState`), putting it away and `Inactive` drop it
  (`StopLockOn`). While it is locked the HUD's lock tone (`HUD_TARGET_LOCK`) loops for
  the player at that PC.
- **The lock-on marker** (`TnObjectiveMarkerTypeLockOn`, the HUD movie's
  `mc_markerLockOn` in `markers_mc`, added as the picked enemy changes and its special
  argument the lock's progress, `GetNormalizedLockOnTime`), run as its script runs it
  (`src\hud.rs`): while the lock comes it eases a quarter of the way to the screen's
  middle each update, its three prongs closing in from 20 out and their `acquiring_mc`
  fading in; locked, it goes over onto the enemy, faster each time (its
  `LockStickiness`, 0.25 up to 1), dims its prongs to a quarter and plays its
  `locked_mc`'s 'on' ('off' as a lock is lost). It isn't kept on the screen, so a
  enemy behind the camera hides it. The jets' rockets now show it too.
- **The rocket** (`TnProjectileHomingRocket`; Escalation flies its `Data`,
  `RocketLauncher_SP_PROJDATA`, as it has no `SurvivalData`): 70 m/s, straight, for up
  to 4 s. Fired locked on it homes on that enemy (`TnProjectileHoming`'s `Homing`
  state, pushed at it at `HomingForce`, 400 m/s², and within `ClosingDistance`, 20 m,
  its `Closing` state at `ClosingForce`, 600 m/s², going off where it is after
  `ClosingTime`, a second); fired without a lock it flies straight
  (`HomeTowardsReticuleWhenNotLockedOn` is off). It goes off on the first thing in its
  way: 375 to every enemy within 7 m (`DamageRadius`), through walls
  (`IgnoreHurtRadiusTrace`), full out to 2.5 m and half beyond (its `RadiusDamageScale`),
  and none of it to Jazz (Escalation's `SelfDamageMultiplier` is 0).
- **Its effects** (`tools\build_weapon_fx.py`): `MuzzleFlash_RocketLauncher_FX` at the
  muzzle with the shot's light (`Brightness` 25 in 245, 174, 122: its
  `TnWeaponEffectLight`'s defaults), the flame out of its back with every shot
  (`RocketLauncherExhaust_FX` on `MuzzleExhaust`), `Tracer_RocketLauncher_FX` flying
  (the rocket's mesh, `PROJ_rocket_STAT`, with its fire and smoke) with its light
  (`Radius` 750, `Brightness` 2, white), `Squib_RocketLauncher_FX` where it goes off
  (at 700/650 of its size: `DamageRadius` over `ExplosionEffectRadius`), and as it reloads dark smoke out of its exhaust (`ReloadSteam_DarkSmke_FX` on
  `ReloadExhaust`) and steam (`ReloadSteam_FX` on `ReloadSocket`).
- **Its sounds** (`BL_WPN_GUN_ROCKET`): the shot, the rocket's flight loop and blast,
  the reload, equip, holster and change hands, `FINE_AIM_START` as fine aim begins and
  as it ends (`RocketLauncher_WEPMESH` has it for both), the lock tone and the
  ballistic dry fire.

It fires as fast as its clip allows (`FireIntervals` 0, semi-automatic): a clip of one,
reloading at once (2 s), 5 in reserve. It comes out in a fifth of a second, its spread
opens from 0.04 by 0.02 a shot to 0.08, back over 3 s (half that in fine aim), with
the game's rocket reticle (`mc_crosshairRocket`), each shot kicks the view up 0.25
degrees (over 0.6 s after a quarter of a second, the same in fine aim) and jolts it
(its `CameraShakes`: intensity 0.6, rolling 0.4 over 0.4 s, over half a second), and
Jazz holds it in the heavy-gun stance (`STANCE_HeavyGunArmShootFromHip`). Its fire
clip is the gun's kick with the arm's `Weapon_Recoil_Heavy_S` at 1.5 times its speed.

**The A-4 Pulsar Cannon** (`TnWeaponGrenadeLauncher`, the "GrenadeLauncher" inside,
`WEP_GrenadeLauncher_p`; `GrenadeLauncher_WEPDATA`, as Escalation has none of its own
for it):

- **The round** (`TnProjectileGrenadeLauncher`; its `Data`, `GrenadeLauncher_SP_PROJDATA`):
  45 m/s, all but straight (its `GravityScale`, 0.001), keeping the way it was fired
  (`bRotationFollowsVelocity` is off). Its `OnHitThing` sticks it in the first thing it
  strikes (it stops, and `AttachToActor`: the level holds it where it touched; an
  enemy on its bone nearest where it struck, riding it as it moves and falls, as on
  another player's), its flight's loop giving way to its fuse's
  (`HitThing`). It goes off when its `FuseTime` runs out, 2 s after it stuck, facing
  back the way it was fired (`HmProjectile.OnWaitingForLifeSpanDestruction`); at once,
  facing up, when its shooter sets it off, when he fires a fourth (the launcher keeps
  three out, its `NumberOfActiveGrenades`: the oldest goes,
  `TnWeaponGrenadeLauncherBase.ServerSpawnedProjectile`), or when the enemy it is in
  is destroyed (its `ShouldDetach`). Each blast: 225 to every enemy within 7.5 m,
  through walls, full out to 3.5 m and half beyond, and a quarter of it to Jazz. Until
  it strikes something it has no `LifeSpan` (here 20 s, after which it goes off where
  it is).
- **Setting them off** (`TryAltFirePress`: `AttemptToDetonate`). It has no fine aim
  (`SupportsFineAim` is off); the right mouse button sets off every round of his out,
  with its detonation sound (fire mode 1's `StartFire`,
  `PULSAR_CANNON_REMOTE_DETONATE`), or with none out its dry click (its `NoAmmoFire`,
  `GRENADE_LAUNCHER.DETONATE_REMOTE_DRYFIRE`). Picking up another gun in its place sets
  them off too (`DropFrom`'s `ServerDetonateGrenades`). While some are out the HUD shows
  the game's DETONATE prompt under the crosshair (`mc_activeGrenadePromptText`, at its
  left, with the right mouse button's icon; the left's in the Southpaw layout:
  `updateIcon`).
- **Its effects**: `Pulsar_MuzzleFlash_FX` with the shot's light (`Brightness` 25 in
  121, 75, 108) (its burning ball of plasma, `Plasma_Ball_Export`, an emitter that
  would loop on for as long as the flash is on, goes out after its first loop, a
  second, as the game turns the gun's flash off, and with the gun when it is put away:
  it no longer burns on the muzzle shot after shot or hangs in the air where the gun
  was swapped), its glow of plasma on the gun while it is ready (`Pulsar_Idle_FX` on
  `IdleFXSocket`, from `ActiveStart` to `ActiveStop`), `Trail_PlasmaCannon_Med_FX` on
  the round (at half its size, the projectile mesh's `DrawScale`, flying and stuck, its
  trail and glow coloured by the side that fired it, as the Gear Shredder's squib; its
  light is black, so it lights nothing), `Squib_Pulsar_Lrg_FX` where it goes off (at
  750/650 of its size), and
  steam as it reloads (`ReloadSteam_Med02_FX` and `ReloadSteam_Med_FX` on
  `ReloadSocket`).
- **Its sounds** (`BL_WPN_GUN_PLASMA_CANNON`): `PULSAR_CANNON_SHOOT`, the round's
  flight loop (`PULSAR_CANNON_SHOOT_TAIL_LP`), its fuse once it sticks
  (`PULSAR_CANNON_PROJ_IMPT_STICK`), its blast (`PULSAR_CANNON_PROJ_EXPL`), the
  detonation, the reload, equip, holster and change hands, and the ballistic dry fire.

A round a pull of the trigger, a second apart at the quickest (the default
`FireIntervals`, semi-automatic), 3 rounds a clip (9 in reserve, 2.5 s reload). Its
spread is a steady 0.01, with the game's cannon reticle (`mc_crosshairCannon`); it
doesn't kick the view (its camera recoil is zero), only jolts it (intensity 0.7,
rolling 0.5 over half a second, over 0.4 s); it takes three quarters of a second to
come out. Its fire clip is the gun's kick with the arm's `Weapon_Recoil_HeavyLight_S`
at 1.5 times its speed.

**The Chaos-Rift Combustor** (`TnWeaponClusterCannon` on `TnWeaponTankCannonArcable`,
the "ClusterCannon" inside, `WEP_ClusterCannon_p`, on the grenade arm's mesh,
`WEP_GrenadeArm_p`;
`ClusterCannon_SV_WEPDATA`):

- **The lob** (`ServerSpawnedProjectile`: `CalculateTossVelocity`, its
  `TnTankCannonArcableExtension`). Each bomb is thrown from the muzzle to what it would
  strike along its aim within the gun's `Range`, 50 m (or 50 m along it), in a flight
  time from 0.1 to 1.1 s by that distance over the range (its `FlightTime`): the throw
  that lands it there under the world's gravity.
- **The bomb** (`TnProjectileClusterCannon`; its `SurvivalData`,
  `ClusterCannon_SV_PROJDATA`): it falls under the world's gravity (`GravityScale` 1,
  `PHYS_Falling`) and goes off on whatever it strikes: 150 to every enemy within 4.5 m,
  full throughout, through walls, a quarter of it to Jazz. However it goes off it
  **bursts into five clusters** (`TnProjectileTankShellArcable.Explode`: its
  `TnClusterBombProjectileExtension`'s `SpawnCluster`): each thrown along a way at
  random within 65 degrees of the surface it struck (`RandomVectorInCone`) at 15 to 25
  m/s (`AdditionalVelocity`). **Pull the trigger again while it is in the air** and it
  bursts there (`StartFire`: `RequestClusterBomb`), the clusters thrown on along its
  path (its velocity, `VelocityTransferScale` 1, plus theirs); the HUD shows the game's
  CLUSTERBOMB prompt while it flies (`mc_clusterBombPromptText`, at the crosshair's
  right, with the fire button's icon; the right mouse button's in the Southpaw layout).
- **The clusters** (`TnProjectileClusterCannonBomb`, grenades, `TnProjectileGrenadeBase`;
  their `Data`, `Clusters_SP_PROJDATA`): falling at 1.25 times the world's gravity, they
  bounce off what they strike keeping 0.55 of their speed (`BounceDampening`), their
  fuses lit as they first strike something (`FuseTime`, at random from 0.5 to 1.75 s)
  and their flight's loop giving way to their fuse's; they come to rest once their
  speed over the last second is under a metre a second (`LowSpeedThreshold`), and go off
  as their fuses run out, or at once on an enemy or a player they strike
  (`ExplodeWhenHittingPawn`): 150 to every enemy within 8.5 m each, full throughout,
  through walls (every cluster counts: `ShouldDamageActor`'s one cluster an enemy is
  only for team games), a quarter of it to Jazz.
- **Its lob shown** (`TryAltFirePress`: `_IsAiming`). It has no fine aim; hold the right
  mouse button while it is ready and its lob is simulated every frame
  (`SimulateTrajectory`: `TnProjectile.RunPhysicsSimulation`, 0.05 s at a time for up
  to 3 s or until it would strike something) with the bomb's
  `TrajectoryPreviewEffect`, `Cluster_Trajectory_FX`, laid along it, and its impact
  marker where it would come down: `Sphere_03_MESH` at ten times its size in
  `MarkTarget_Depth_MAT`, a modulating material that glows yellow where the sphere
  meets the ground (the builder writes it as a system of its own,
  `ClusterCannon_Marker_FX`), turned into the surfaces the lob struck over the last
  quarter of a second (its `_NormalHistory`). Letting go, or the gun leaving `Active`
  (a shot, a reload), stops it.
- **Its effects**: `ClusterCannon_MuzzleFlash_FX` with the shot's light (its
  `TnWeaponEffectLight`'s defaults), a spent shell flung out glowing with every shot
  (`Lrg_Shell1_FX` on `ShellEject`), the bomb, its mesh (`PROJMESH_GrenadeBall_STAT` in
  `PROP_NEU_MineBall_MATINST`) trailing `ClusterCannon_Tracer_FX`'s dark smoke, with its
  light (`Radius` 1500, `Brightness` 8, white), `ClusterCannon_ExplosionPrimary_FX`
  where it goes off (450/375 of its size), the clusters (`PROJMESH_Grenade_STAT` in
  `WEP_ClusterCannon_cans_MATINST`, and the same smoke, both 1.5 times their size: the
  mesh's `DrawScale`), `ClusterCannon_ExplosionSecondary_FX` where each goes off
  (850/375), and steam as it reloads (`ReloadSteam_FX`
  on `ReloadFX`). The bomb and the clusters are drawn as systems the builder writes, one
  still mesh particle each (`MESH_SYSTEMS`), turning with them.
- **Its sounds**: the shot (`GRENADE_LAUNCHER.SHOOT_LOW_AMMO`, which its
  `TnWeaponEffectLowAmmoSound` plays for every shot), the bomb's flight loop and blast
  (the tank cannon's `TRAIL_LP` and `EXPL_IMPT_WORLD`), the clusters' flight loop,
  bounce (`GRENADE_FOLEY_SHELL_BOUNCE_HEAVY`), fuse and blasts
  (`TANK_CANNON.CLUSTER_BOMB_PROJ_LP`, `_FUSE`, `_EXPL`), the grenade launcher's
  reload, equip, holster and change hands, and the plasma dry fire.

Automatic (`AutoFire`), a bomb every half second at the quickest; a clip of one
reloading after each (2.5 s), 5 in reserve. Its spread opens from nothing by 0.04 a
shot to 0.04, back over 2 s, with the game's mortar reticle (`mc_crosshairMortar`); no
kick (its camera recoil is zero), a jolt (intensity 0.3, rolling 0.65 over 0.85 s,
over 0.8 s); it comes out in a fifth of a second. Its fire clip is the gun's kick with
the arm's `Weapon_Recoil_HeavyLight_S` at twice its speed, with every shot.

**The blasts' camera shakes** (their data's `TnCameraShakeProjectileExtension`, played
by `TnProjectile.PlayLocalCameraShake` for the players at that PC): a player further
from a blast than its `DamageRadius` but within its `CameraShakeRadius` has his view
shaken by its `ScreenShakeStruct` (`HmCameraModifierScreenShake`, UE3's
`UpdateScreenShake`: waves of turn, place and field of view, each from a place at
random and easing out over its time), scaled down by his distance (its `UseFalloff`:
its time, sizes and speeds times 1 - distance/radius). The rocket's reaches 150 m for
half a second, the Pulsar's 300 m for a second, a cluster's 150 m for 0.75 s (widening
the view slightly); the Combustor's bomb has none. The camera adds them up
(`src\main.rs`'s `PlayingShake`).

**What changed for every round.** Rounds now stop on the other players as on the
level and the enemies (the game's players stop projectiles,
`TnPlayerPawn.StopsProjectile`, and a round passes through only the player who fired
it, `Projectile.Touch`'s `ShouldIgnoreInstigator`), so the Riot Cannon's rounds, the
tanks' shells, the jets' and trucks' rockets and the slime go off on a teammate in
their way (their blasts still hurt only enemies and the one who fired them, as
before). A round that flies its whole `LifeSpan` now goes off where it is, facing back
the way it flies (`HmProjectile.OnWaitingForLifeSpanDestruction`, its flight effect
alive), rather than vanishing. The jets' rockets fired locked on now home on the other
players' PCs too.

**Online**, the other players' PCs are told of each round fired with its target if it
homes (`HomingRound`: the enemy there nearest where his was), each lob with its
velocity (`Lob`) and each setting off (`SetOff`), and their copies fly, stick, bounce,
burst and go off there (the clusters' ways and fuses drawn from the shot, the same on
every PC). The network protocol is now version 19, so every PC needs this version.

Their meshes (`RB_RocketLauncherR_SKEL`, `RB_GrenadeLauncherR_SKEL`,
`RB_GrenadeArmR_SKEL` and their left-arm meshes), their animations
(`Weapon_RocketLauncher_ANIM`, `Weapon_GrenadeLauncher_ANIM` and `WEP_GrenadeArm_ANIM`:
fire, idle, reload, equip, holster and change hands) and their texture sets
(`WEP_RocketLauncher_TEXSET`, `WEP_A4PulsarCannon_TEXSET`, `WEP_ClusterCannon_TEXSET`)
come in as the Titan's guns' do (`build_assets.py`; export them the same way), and the rounds'
meshes for their effects with umodel's glTF export of `PROJMESH_GrenadeBall_STAT`,
`PROJMESH_Grenade_STAT` and `Sphere_03_MESH` (see `build_weapon_fx.py`).

Their upgrades are in (see the weapon upgrades above). Left out: the push their blasts give
(`MomentumTransfer`); the line the Destroyer says as he fires the Thermo Rocket Cannon
(the incidental dialog's `FiredHomingRocket`, which the multiplayer's voices don't
answer: his grenade launchers' `FiredGrenade`, which they do, is said, see *The
characters' voices*); and the weapon challenges the game counts for them (and the
experience they bring).

### The HUD

Fall of Cybertron's HUD is a Scaleform (Flash) movie, `UI_GFxHud_p.Hud_GFX` in
`TransGame.xxx`: clips with timelines, bitmaps, morphing bars and text in the game's
fonts, driven by ActionScript. There are no 3D models in it; its 3D look comes from
the movie's own perspective settings (a 45-degree view, corner groups turned a few
degrees towards the player, the damage arrows on a ring tilted back 50 degrees),
and the demo draws it the same way. `tools\build_hud.py` reads the movie straight
from the package and writes:

```
assets\hud\hud.txt          the clips, shapes, bars, text fields, frame labels and stops
assets\hud\img\*.png        the movie's bitmaps, weapon icons, grenade icon, low-health effect,
                            controller buttons
assets\hud\font_*.png        glyph atlases of the HUD's fonts (from Startup_int.xxx)
```

The movie's bitmaps are separate textures (`Hud_GFX_I*`), so export them first with
umodel, then run the script:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\hud TransGame.xxx
cd Documents\foc_esc\tools
python build_hud.py "C:\...\Transformers Fall of Cybertron\TransGame\CookedPC" C:\hud\TransGame\Texture2D ..\assets
```

`src\hud.rs` plays the clips the way Flash does (30 frames a second, labels, stop
frames, colour transforms) and does what the movie's scripts do, fed from
`HudState`; `assets\shaders\hud.wgsl` applies Flash's colour transforms. The
scripts were read with `tools\avm1_dis.py`; `tools\gfx_render.py` and
`tools\hud_preview.py` draw the movie to a PNG for checking. The movie's controller
buttons are pictures it imports from another movie, the button icons
(`UI_GFxButtonIcons_p.ButtonIcons_GFX`, also in `TransGame.xxx`), and `build_hud.py`
takes them in with it (`img\Gamepad_*.png`) for the HUD's own button spots while a
controller plays: RB on the ability, X on the reload prompt, LT and RT on the grenade
prompts (see *Playing with a controller*). The contextual prompts name their key in
the text ("Hold E to pickup ..." where the game draws its X button icon; at a health
station "Hold E to purchase Health for \*200", with the price in the energon numbers'
font and gold as the game's `PickupPurchasable` text marks it up), and the hit
ping is silent because its sound isn't in the game's packages. The Photon Burst
Rifle brings its own parts of the movie: the tactical reticle
(`mc_crosshairTactical`), the scope shown while fine aiming (`mc_tacticalScope` in
`scopeManager_mc`: the range finder sliding up its scale with the distance to what
you aim at, "MAX" past 150 m, and the compass strip, cut to its window by the
movie's mask) and the contextual-command line with its glow (`notifyManager_mc`).
The Scatter Blaster brings the shotgun reticle (`mc_crosshairShotgun`, the one the
movie gives every `TnWeaponShotgun`): three glowing arcs pushed out by the spread
and stretched to 10 + spread x 400 pixels across, as its script does, and its
icon, `ItemWireframeShotgun`.
The Nucleon Charge Rifle brings the sniper scope (`mc_sniperScope`, which
`scopeManager_mc` attaches for its fine aim at 120% of its size, and plays out as fine
aim ends) and its icon, `ItemWireframeSniperRifle`; it has no reticle. The scope
shows the charge as its `updateSniperChargeLevel` script does: the percentage in its
readout and on the four bars round its rim, its readout, inner line and rim playing
"ready" as it reaches 100% and "charging" as it starts over (0% while the rifle is
between shots), and its rim and lines turn red over an enemy (`updateCrosshairColor`).
The charge upgrade's three lights (sprite 286's `updateSniperChargeUpgrade`) show with
Deadeye: lit while a hit has it charging faster, out when that ends; without the
upgrade they stay hidden. With the Cyber Tactical Scope upgrade the Photon Burst
Rifle's scope shows its target panel (sprite 336: the name and health of what you aim
at, `TnHudDataObserverTargetName` and `TargetHealthNormalized`) and its range finder's
distance; without it, as a team match has it, the panel stays hidden and the range
finder reads nothing ("—m": `ToggleWeaponSpecificHud` turns their observers off).
Lights Out's round flashes its segment of the ammo bar (`SpecialShotInClip`). The
ammo bar's segments are made as `mc_ammoBarSegmented`'s `updateAmmo` makes them (one
a round for clips of up to 40, overlapping across the bar's 170 pixels), and drawn
as the game draws them: their parts (`mc_ammoBarSegment`'s background and highlight,
which it stretches to the segment's width) have a scaling grid
(`DefineScalingGrid`, which `tools\build_hud.py` writes into `hud.txt` as `grid`
lines), so a longer segment keeps its soft ends a few pixels wide and stretches only
its middle. A one-round clip's one segment (the Thermo Burst Rockets', the
Mega-Concussion Cannon's, the Thermo Rocket Cannon's, the Chaos-Rift Combustor's) is
the whole bar, full while it is loaded, as in the game. The bar is made again when
the clip's size changes (an Ion Cartridge's), as `AmmoBarIsSegmented`'s watch and
the segments' `updateAmmo` make it again. The movie's other parts with scaling grids
(the health and shield bars' glow, the weapon name's backing, the hit pings and some
reticles' lines) are drawn 9-sliced the same way wherever they are scaled. The
experience awards are the movie's point transactions (`mc_pointTransaction` and
`mc_pointTransactionLabel` under topCenter_mc, sprite 894's queue: each award shown 2 s,
a kill's awards adding up in one "+N", the next transaction 0.2 s after the last has
gone, each fading out rising 100 pixels), and a new level its level-up notice
(`mc_levelUpNotify` in midRight_mc, with the class's icon from the shared icons movie,
`mc_specialtyIcon*`, which the export draws as pictures: the HUD movie names all four
but imports only the Infiltrator's). The Path Blaster brings the pistol reticle
(`mc_crosshairPistol`): the rifle's three prongs, which its script pushes out by
spread x 400 pixels rather than 500, and its icon, `ItemWireframeHeavyPistol`. The Riot
Cannon brings the cannon reticle (`mc_crosshairCannon`, the one the movie gives
`TnWeaponBazooka`), pushed out by spread x 400 pixels too, and its icon,
`ItemWireframeBazooka`. The Titan's weapons bring their icons
(`ItemWireframeHeavyMG`, `ItemWireframeDrainGun`, `ItemWireframeChainBeam`,
`ItemWireframeAoESlow`), the E.D.K. TechVolt its beam reticle
(`mc_crosshairChainBeam`: three prongs pushed out by spread x 500 pixels, spinning in
as the gun comes out, as the rifle's do) and the Corrosive Slime Cannon the mortar
reticle (`mc_crosshairMortar`: its two prongs pushed out by spread x 300 pixels and its
lines by a fifth of that, its own timeline playing in as the gun comes out); the X18
Scrapmaker has the rifle's reticle and the Energon Harvester the shotgun's, as the
movie's `updateCrosshair` gives them. The Gear Shredder brings its icon
(`ItemWireframeSawBlade`) and the saw-blade reticle (`mc_crosshairSawblade`: two prongs
pushed out sideways by spread x 400 pixels and stretched as they go, its own timeline
playing in; it has no spread, so they stay in). The Destroyer's new weapons bring
their icons (`ItemWireframeHomingRocket`, `ItemWireframeGrenadeLauncher`,
`ItemWireframeClusterCannon`): the Thermo Rocket Cannon the rocket reticle
(`mc_crosshairRocket`), the A-4 Pulsar Cannon the cannon reticle and the Chaos-Rift
Combustor the mortar reticle, as `updateCrosshair` gives them; the lock-on marker
(`mc_markerLockOn`) and the DETONATE and CLUSTERBOMB prompts under the crosshair
(`mc_activeGrenadePromptText`, `mc_clusterBombPromptText`) are the movie's own, run as
its scripts run them (see *The Destroyer's weapons*). The ammo warnings are the movie's own (`updateAmmoDisplay`,
`updateAmmoWarnings`): the ammo bar and its number turn red once a quarter of the clip
or less is left in it (`LowClipValue`), and the low-ammo warning, the red glow and
the weapon's name in red come once everything left, clip and reserve together, fits
in one clip. The vehicles' guns show their names as the robots' guns do (the game's
`FriendlyName`s: M1 Tachyon Repeater, Thermo Burst Rockets, Mega-Concussion Cannon,
Eclipse Rockets), with no icon of their own. Ammo picked up is listed over the ammo
count with each weapon's small icon (`updateAmmoAdded`: its `getLinkageId` turns the
weapon's class `TnWeapon<X>` into the kill feed's `GameMessageIcon<X>`, a vehicle's
gun into its vehicle's; `tools\build_hud.py` reads them from `TransGame.xxx` when the
umodel export hasn't got them). The game-type announcement at the start is the movie's
`announcement_mc` line run as its `updateGameAnnouncement` script does
(`HudEvent::GameAnnouncement`). The other game modes put their names there as a
match begins ("Team Deathmatch", "Conquest"...), but the game leaves Escalation's empty
(`TnGameTypeMessageSV` has no `GameTypeMessage` in `TransGame.int`), so the demo
uses the mode's name; it is `GAME_TYPE_MESSAGE` in `src\menu.rs`. The shield and health
bars are bottomLeft_mc's `shieldBar_mc` and `healthBar_mc`, run as its
`updateShieldHealth` script does. In a match each other player has the movie's
teammate marker over his head (`mc_markerPawn` in `markers_mc`, run as its
`updateSpecialArguments` script does): his name in the friendly colour over his
shield and health bars, which turn red with a red glow behind them at 75% health
and under with the shield down, or half health with some shield left
(`HudState::allies`, fed by `feed_hud_allies` in `src\main.rs`). The game also
pins a teammate's marker to the screen's edge with an arrow when he is off the
screen; the arrow is a vector shape the HUD export can't draw yet, so the demo
hides the marker instead. An enemy the Orbital Pistol has revealed has the same
marker as an enemy's (`HudState::revealed`; see *The Orbital Pistol*): in the enemy
colour, with no name or bars, so the marker's arrow over it shows (`icon_mc`, the
chevron that unfolds as it appears: the HUD export draws this one shape as a
picture, `img\HudShape667.png`, as `build_hud.py`'s `RASTER_SHAPES` says, since it
isn't a rectangle) with its mark's icon on it (`buffs_mc`, which `updateBuffs` fills
with `mc_markerPawnBuffIcon` clips holding the buffs' bitmaps:
`BuffIconHardLocked`, `2` and `3`). Escalation's waves (see *Escalation's waves*)
bring the countdown's clock and the wave's line, both the movie's own: the clock is the
single-team scoreboard's (`mc_scoreboardSingleTeam`, which the top right attaches),
sliding in from the right as it fades in and out the same way as the wave starts, each
second from 10 down flashing red (its `updateShowCountdown` and
`updateCountdownTime`); the wave's line, "WAVE 1 OF 15", is `mc_currentWaveMessage`
under the energon counter, typed in a letter at a time (`updateCurrentWave`). The
wave's announcements ("Wave 1 Begins In...", "Five..." and the rest) type in at the
top like the other announcements. A wave's last few enemies get the enemy marker
through walls (the game's `TnBuffSeeEnemyObjectiveMarkers`), the same marker as an
enemy the Orbital Pistol reveals. Plain text fields now take the font the movie gives
them, so the experience awards' "+N" are in its number font, as in the game. Without
`assets\hud` the demo runs without a HUD.

### The kill feed

Every death in a match gets a line at the HUD's top left, as in the game: who killed
whom and with what, "Sentry", the sentry icon, "Soldier"; your name, the tank icon,
"Scrapper"; "Shotgunner", the shotgun icon, your name. The words are the game's death
messages (`TnDeathMessage`, and each damage type's `DeathString` in `TransGame.int`,
taken from the nearest of its classes that has one), with the icons in place of their
bracketed words as the HUD's message list puts them in (its
`GameMessageIconSubstitutionArray`: each at 80% of its size, on the text's line), and
each name in its colour as you see it (`GetColorForPRI`): yours in the player colour,
your teammates' and the sentries' in the friendly colour, the enemies' in the enemy
colour, and the rest of the line in the killer's. A death with nobody's hand in it (a
trap's flames or blast, bleeding out while down, giving up with G) is the damage type's suicide line instead, the icons on the name; a line
with no killer and not about you is drawn at 60%, as the game does. The list keeps
five lines, each for five seconds (`MaxMessageCount`, the lines' `fadeOut`).

A kill is named by the blow that made it: your guns, vehicle guns, melee, Hover Slam,
Whirlwind, mines, Dimensional Decimator, Fireblast and its burn; the enemies' guns,
blows, grenades, rockets and a burnt enemy's blast (each its own damage type, from the
AI's data); the traps' sentries and Ignition's gunship ("Sentry", as the game names a
sentry's kills after the sentry, which goes for the Titan's Spawn Sentry too). Online,
each death is noted on the PC it happens on (an enemy's where the killing blow was
struck, a player's on his own) and goes to the others, each showing it as its own
player sees it; the network protocol went to version 37 for it, as an enemy's blow to a
player now says its damage type. The words and the damage types are in
`src\killfeed.rs`; the icons the ammo notes didn't already have
(`GameMessageIconSentry`, `Melee`, `Suicide`, `FireDot` and the rest) come from
`tools\build_hud.py`. Not in: the death message's headshot line (`HeadShotString`);
a headshot kill reads as its gun's.

### The main menu

The game's front end is the Scaleform movie `UI_GFxFrontEnd_p.FrontEnd_GFX` (its
`menu_Main` clip), with the settings menu `UI_GFxSettings_p.SettingsMenu_GFX`, both
in `UI_FrontEnd_m.xxx`; behind it the game plays the Bink movie
`Movies\BG_RustSea.bik`. The Extras menu (`UI_GFxExtras_p.ExtrasMenu_GFX`, in the
same package) plays the ending credits, `Movies\TFOC_Ending_Credits.bik`.
`tools\build_menu.py` builds the demo's menu from them:

```
assets\menu\background.h264   BG_RustSea re-encoded as H.264 (it loops seamlessly)
assets\menu\credits.h264      TFOC_Ending_Credits re-encoded as H.264 (2:40, about 28 MB)
assets\menu\credits.ogg       its music (the Bink's left and right tracks) as Ogg Vorbis
assets\menu\img\*.png         the logo, brackets, settings art, slider and button icons,
                              the Extras menu's selection bar
assets\menu\fonts\*.ttf       the movie's fonts (Register Demi Bold, Medium and Light) as TrueType
assets\menu\menu.txt          the menu's text from the game's Coalesced.int, and font widths
```

The text comes from the game's own English strings (UIText.int, packed and
scrambled inside `Localization\Cooked\PC\Coalesced.int`; the script unscrambles
it). Export the movie's images with umodel, then run the script on the
`TransGame` folder (it needs Python with `fontTools` and `Pillow`, and `ffmpeg` with
libx264):

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu UI_FrontEnd_m.xxx
cd Documents\foc_esc\tools
pip install fonttools pillow
python build_menu.py "C:\...\Transformers Fall of Cybertron\TransGame" C:\menu\UI_FrontEnd_m\Texture2D ..\assets
```

`src\menu.rs` lays the menu out in the movie's 1120 x 720 stage, scaled to the
window as Scaleform does, using the positions, colours and animations of the
movie's scripts (read with `tools\avm1_dis.py`): the logo fading in, the menu
sliding into place and its items fading in one after another, the focused item
sliding left as its brackets open (each label drawn at its own size inside its
button, which the movie stretches 105% by 109%: the game's CLIK buttons keep their
text unscaled, so the brackets, which do take the stretch, sit centred around it),
the description fading in under it, and the
items turning away and back as a sub-menu opens and closes. The button hint
under the menu (the gamepad's A button, Select) shows only while a gamepad is
plugged in. The background movie is decoded on its own thread (the `openh264`
crate) and, as in the game, stops and darkens behind the settings menu. The list
leaves out CONTINUE and CAMPAIGN; the five items left take the rows from
CAMPAIGN down, so the list ends where the game's does.

The settings menus follow the settings movie's `mc_menu` and its menus as the
PC version lists them (`RootMenuProvider`: Display, Graphics, Audio, Controls):
rows 27 pixels apart, centred on the menu's anchor, with the selection bar
following the focus and the whole menu drifting slightly with it; the focused
row's label darkens and slides left; the title types each menu's name over the
last one; a sub-menu replaces its parent and Back returns to it with the same
row focused. Graphics, Fullscreen Mode, VSync and the Framerate Cap are the
movie's `MenuItemStepper`s (focused, the values show side by side, the chosen one
bright; otherwise only the chosen one). The Back button is drawn as the PC version
shows it, a box under the menu. Graphics holds the game's rows
(`GraphicsMenuProvider`: Graphics, Fullscreen Mode, Resolution, VSync) and the
demo's Render Scale and Framerate Cap (see *Graphics settings and performance*).
Audio
has a Master Volume of the demo's own over the game's three sliders
(`AudioMenuProvider`'s FX Volume, Dialog Volume and Music Volume, 0 to 100 in
steps of 5, all 100 until they are set; its Subtitles row is left out, the demo
having nothing to subtitle). Every sound follows Master Volume and the slider of
its class, from the game's mixer category for its cue (see Sound effects):
dialogue is the announcer, music the menu music, the match-start stingers and the
ending credits, and the rest is effects (the garage's ambience and the car's
engine among them). Sounds already playing follow the sliders as they move, and
each slider plays its own sound at its new volume as it moves, as the game's do.
Controls is the game's (see The Controls settings). Display has the game's three
rows (`DisplayMenuProvider`): **Brightness** opens the game's brightness screen
(`Brightness_GFX` over `CalibrationImage_GFX`): the calibration picture in its
glowing frame fading in, the game's slider under it (0 to 100, 50 until it is set;
Left and Right, its arrows or a drag, a step of 1 that repeats while held, with the
game's calibrate sound) and "Adjust brightness so the left emblem is barely
visible." The brightness is the game's display gamma (`GetGammaSetting`: 2.2 at 50,
0.95 lower at 0 and higher at 100), and the game takes it up as the slider moves:
the maps are drawn through their colour grade at that gamma, and the Customize
garage through the game's tone curve at it, so the world brightens or darkens behind
the menu as you set it (the HUD and menus keep their own colours, and the
calibration picture is drawn as the gamma would show it). Once it has moved, Accept
(Enter, the gamepad's A, or a click) keeps it with the game's sound; Back (Esc)
puts back the brightness the screen opened with. **Safe Frame** opens the game's
safe frame screen (`SafeFrame_GFX`): the glowing safe area over the whole window
with an arrow at each edge, and "Move the arrows to adjust the safe frame to fit
your display." on its band. Left and Right narrow and widen it, Up and Down raise
and lower its top and bottom edges (half a percent of the screen at a time,
repeating while held, from the full window to 18% in from it, as the game allows),
the arrows flashing as they step with the game's sound; Accept keeps it and Back
leaves the frame as it was. The HUD's corners sit at the safe frame (0 on PC until
it is set: the window's edges). **Team Coloring Mode** is the game's stepper,
**Ally vs Enemy** (the game's default) or **Autobot vs Decepticon**, each word in
the game's colour; as in the game it colours the HUD's messages (the kill feed's
names blue for you and your team, or your faction's red or purple), while the robots
glow in their faction's colours either way: on the Customize screen, as your game
shows it, and in an Escalation match, the game's single-team mode (the game colours
them by team, cyan as allies, only in a team game or a lobby). All three are saved
in `settings.txt`. The demo adds a fourth row, **Split Screen**: **Top and Bottom**
(until it is set) or **Side by Side**, how the window is shared when two play on
this PC (see *Split screen*), saved as `split_screen`.
Fullscreen fills the screen: at the Resolution chosen, in that video mode
(exclusive fullscreen, as the game's), or at Desktop with a borderless window. The
Framerate Cap holds each frame back until 1/cap seconds after the last (60 until
it is set; None lets it run free); with VSync off, frames are shown as soon as
they are done, using the driver's tear-free Mailbox mode where it has one. The
settings are saved in `settings.txt` next to the `assets` folder.

### Graphics settings and performance

**The Graphics level** is the game's `GraphicPreference` row (`GraphicsSettings`:
Low, Medium, High and Extra), kept in `settings.txt` as `graphics_quality`. In the
game it sets the textures' detail (its `PCSettingManager`'s LOD biases: Low skips
two mip levels of the world's textures and one of the characters', Medium one of
the world's, High and Extra none) and, changed, says "Game Restart Required:
Restarting the game is required for the texture quality settings to take effect."
The demo does the same (the notice comes as you leave the Graphics menu) and also
sets the rest of its graphics as a group (`src\graphics.rs`):

| | Low | Medium | High | Extra |
|---|---|---|---|---|
| Sun's shadow map, cascades, reach | 1024, 2, 60 m | 2048, 3, 120 m | 4096, 4, 220 m | 4096, 4, 320 m |
| Bloom / depth of field / motion blur | off / off / off | on / off / off | on / on / on | on / on / on |
| Anti-aliasing | none | FXAA | 4x MSAA | 4x MSAA |
| Lights shots and blasts make | none | the 6 nearest | all | all |
| Effects' detail (levels dropped) | 2 | 1 | 0 | 0 |
| Effects drawn out to | 60 m | 120 m | any distance | any distance |
| Effects moved every frame in a busy fight (the rest, further off, every other frame) | nearest 40 | nearest 70 | nearest 100 | all |
| Characters' shadows (lights other than the sun) | none | 3 nearest | 6 nearest | 10 nearest |
| Texture mips skipped (world, characters) * | 2, 1 | 1, 0 | 0, 0 | 0, 0 |
| Anisotropic filtering (maps, effects) * | 1x, 4x | 2x, 8x | 4x, 16x | 16x, 16x |

\* from the next start. High is the demo's look before the setting (the
default; only its far effects in a busy fight move every other frame); the
effects' detail is the game's own levels of detail (each effect's
`LODDistances`, used that many levels nearer: UE3's `ParticleLODBias`), though
never a level an effect keeps for further off than that setting draws effects at
all (60 m on Low, 120 m on Medium).

**Resolution** is the game's slider (`GraphicResolution`, its value shown beside
it as the game's resolution label is): the primary monitor's resolutions (800x600
and up), then **Desktop**. **VSync** is the game's: on, each frame waits for the
display; off (until you set it), frames show as soon as they are done. As in the
game, Fullscreen Mode, Resolution and VSync take effect together with **Accept**
(it shows beside Back once one of them has changed; Enter or the gamepad's A
accepts too), and Back leaves them as they were. They are kept in `settings.txt`
as `fullscreen`, `resolution` and `vsync`.

**Render Scale** (the demo's own, 50 to 100% in steps of 5, kept as
`render_scale`) draws the game at that share of the window's width and height
and spreads the picture over the window (filtered), under the HUD and menus,
which stay at the window's own resolution. At 100% the game is drawn straight
into the window, as before (split screen's two views excepted: they have a picture
of their own the window's size, see *Split screen*).

**The characters' shadows from the lights other than the sun** follow the game's
light environments (`src\shadows.rs`). Every form of a character has one in the
game (`TnRobotForm`'s `LightEnvironment0`, `TnVehicleForm`'s `MyLightEnv`, ...),
and the PC version has `LightEnvironmentShadows` on: the environment gathers the
level's lights round the character (each checked for walls in the way) and casts
one soft shadow away from the strongest direction they come from (never from lower
than 35 degrees over the horizon, its `MinShadowAngle`), as dark as that
direction's share of the light. The sun keeps its own shadows, and where it
reaches a character its shadow is the character's only one. The game casts the
environment's shadow as a projected shadow of the character's own model. Here each
character near the camera (players and enemies, the nearest few by the Graphics
level) does the same: a spot light of its own that gives no light stands 20 m off
the way the shadow's light comes from, and draws a shadow map of that character
alone (its meshes on a render layer only that light's shadow sees). The shadow is
drawn as a soft dark shape over whatever it falls on, looked up in that map, fading
out by 45 m from the camera, and left out where the sun's shadow map shows the sun
reaching the character (`sun_probe`). The character's body as capsules (an enemy's
hit shapes, a player's ragdoll bodies, a vehicle's capsules fitted to its model)
says where it is and how big, and which of the picture is the character itself.

**Performance.** `Cargo.toml`'s release profile builds the engine and its
libraries in one unit each (the slowest to build, the quickest to run; they are
built once) and the demo's own code incrementally, so an update rebuilds quickly;
`boot demo cargo.bat` plays that build. In a big wave the frame's costs were
measured on the PC (the `foc_esc_render` harness's profile: each system's time a
frame, and the GPU's passes): the graphics card has room to spare (drawing at a
quarter of the pixels made no difference), so the frame is the processor's. These
costs were cut:
- **Sounds:** Bevy plays a recording by opening its Ogg Vorbis file afresh each
  time (its codebooks set up, about a millisecond), and a fight starts several a
  frame: 3 to 5 ms a frame. Each recording is now decoded into memory the first
  time it plays (on a worker thread; the short ones, up to 160 KB, the footsteps,
  shots and blasts) and played from there afterwards.
- **Effects from one buffer:** every effect's particles (and the lens flares) are
  drawn anew each frame. Each was a mesh of its own, and every changed mesh is sent
  to the graphics card again with its bounds and draw worked out anew: in a big
  fight a few hundred a frame, about 2 ms of the render thread's frame. They now
  all draw from one buffer of the frame's effect data, written to the graphics card
  whole once a frame, and their meshes only number their vertices for the shader
  to read them from it (`FxWords` in `src\levelfx.rs`).
- **Effects out of view:** an effect whose sprites are out of the camera's view
  hides and keeps what it drew as it was (it goes on running, and is drawn again as
  it comes into view): its vertices aren't made, copied or drawn.
- **Far effects in a busy fight:** drawing an effect anew (its particles sorted and
  turned to the camera, its vertices made and copied) takes the main thread's
  time. With more effects in view than the Graphics level's number (High: 100),
  those further off than the nearest that many are drawn anew every other frame,
  half one frame and half the next, and are drawn as they were in between (they go
  on running every frame; one that comes into view is drawn at once). In the wave
  above, about 75 of some 250 effects a frame wait a frame (`FxBudget` in
  `src\levelfx.rs`).
- **The HUD's draw order:** each of the HUD's pieces has its place in the drawing
  order in its material, and a damage arrow or hit marker attached partway down
  the list renumbered every piece after it: about 30 materials a frame in a fight,
  each prepared again for the graphics card. The pieces now keep their numbers
  while they are still in order (spaced so that new pieces fit between), and only
  the new ones are numbered (`draw_orders` in `src\hud.rs`): the HUD's changed
  materials went from about 45 a frame to about 25 (the rest are its own colour
  animations).
- **The HUD's colours:** those were the pieces' colour transforms (their fades and
  flashes), each in its piece's material. They are now in one buffer, each
  piece's at its own place (its mesh tag), copied to the graphics card as they
  change for the HUD's shader to read: the HUD's changed materials went from
  about 18 a frame to 2 or 3, about 0.4 ms of the render thread's frame
  (`HudColors` in `src\hud.rs`).
- **Materials changed every frame:** a changed material is prepared again, and Bevy
  makes anew the bind group of the group of materials it is in (on the maps, one of
  many textures): the map's pulsing glows, the outlines' flares, the gun effects'
  fading sprites and the characters' shadows changed theirs every frame, about 1 ms
  of the render thread's frame in all. They are now worked out in their shaders or
  read from buffers, and no material changes from frame to frame (`LiveMaterial` in
  `src\live.rs`, `SpriteLooks` in `src\fx.rs`, `SlotShadows` in `src\shadows.rs`).
- **Bindless materials:** Bevy keeps the standard materials in a few big shared
  groups of textures, and wgpu goes over every texture in a group each time a pass
  binds it (40 to 70 µs for the maps'), which the passes did some 65 times a view
  each frame: about 2.5 ms of the render thread's frame for every view. The demo
  turns buffer binding arrays off, so each standard material has a bind group of its
  own few textures, about a microsecond to bind (`wgpu_settings` in
  `src\graphics.rs`).
- **Light-mapped meshes:** Bevy makes its light-map slots' bind groups anew in every
  render phase every frame, four meshes to a slot group. The maps' parts are joined
  into meshes that draw alike, mirrored parts now with their unmirrored twins and
  all of a map's light-map pages on one atlas: about half as many meshes as before,
  some 150 fewer bind groups a frame on Oblivion, about 1 ms of the render thread's
  work (`src\merge.rs`).

The AI (about 2 ms a frame for 14 enemies) and the enemies' animation (under 1 ms)
were measured too and run alongside the drawing, which takes longer, so thinning
them would not make the frames shorter on this PC; they are left as the game has
them. Lower Graphics levels cut the drawing's cost further (fewer, nearer and
simpler effects, fewer lights and shadows).

### The Customize screen

The game's multiplayer character customization is the Scaleform movie
`UI_GFxCustomCharacter_p.CustomCharacter_GFX` (in `UI_Lobby_m.xxx`), played over
the garage level `UI_CustomCharacter_m`, whose Kismet spawns a preview character
on each of its four platforms and whose camera moves between the views of
`CAM_UI_Strategies_p.Customization_STRATEGY` (in `TransGame.xxx`). Two scripts
build the demo's copy:

```
assets\models\garage.glb              the garage: every placed mesh, light-mapped like Downfall
assets\levels\garage_lightmaps\       its light map pages, and beside each (*_dir.png) the directions its light comes from
assets\levels\garage_lights.txt       its 32 lights, height fogs and bloom (lighting.rs's format)
assets\levels\garage_fx.txt           its effects: the beams' and glows' materials, lens flares and steam,
                                      and the materials of its surfaces (levelfx.rs)
assets\shaders\levelfx\garage.wgsl    those materials, compiled from the game's (put into shaders\levelfx.wgsl)
assets\textures\levelfx\garage_cube.ktx2   the cube map the game captured in it, which its surfaces and the preview robots reflect
assets\levels\garage_points.txt       the four preview spots, the glows and the 30 camera views
assets\customize\img\*.png            the movie's images (and the CLIK and icon images it imports)
assets\customize\customize.txt        its text, and the game data its lists show (built into the program: build again after changing it)
assets\models\robot_cust.glb          Jazz's customization pose (Cust_Idle) and his idle with a gun (Nav_Idle_Shoot) for robot.glb
assets\choose\img\*.png               the Choose Character screen's images (below)
assets\pregame\img\*.png              the starting scoreboard's images (build_pregame.py, below)
assets\choose\shapes.txt              where its vehicle icons and class list's vehicles go
```

Export the garage and the movie's images with umodel, then run the scripts
(`build_garage.py` on the export and `CookedPC`; `build_customize.py` on the
`TransGame` folder, the animation sets exported for `build_assets.py` (it takes
`Cust_Idle` from `Shared_ROBO_ANIM` and `Nav_Idle_Shoot` from
`Shared_SCOUT_ROBO_ANIM.psa`) and the image exports; the hint bar's controller
button pictures it reads from `TransGame.xxx` itself, and `--pad-icons <TransGame
folder> <assets>` writes just those):

```
umodel -game=trans -export -gltf -png -path="C:\...\TransGame\CookedPC" -out=C:\garage UI_CustomCharacter_m.xxx
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\cust UI_Lobby_m.xxx
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\cust UI_FrontEnd_m.xxx
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\cust TransGame.xxx
cd Documents\foc_esc\tools
python build_garage.py C:\garage\UI_CustomCharacter_m "C:\...\TransGame\CookedPC" ..\assets
python build_customize.py "C:\...\Transformers Fall of Cybertron\TransGame" ..\assets "..\..\TFOC Project\umodel\UmodelExport\Animations" C:\cust\UI_Lobby_m\Texture2D C:\cust\UI_FrontEnd_m\Texture2D C:\cust\TransGame\Texture2D
```

`customize.txt` is built into the demo as it is compiled (`src\customize.rs`'s
`DATA`), so that the unlocks and prices can't be changed by editing it: the copy in
`assets` isn't read. Build the demo again after `build_customize.py` has changed
it (cargo does so by itself when it has).

The garage's meshes keep their textures from the packages its props come from
(umodel finds them in `CookedPC`), and its light maps and lights come from
`UI_CustomCharacter_m_LM.xxx` as Downfall's do. Its effects are the game's, drawn
the way the maps' are (`tools\levelfx.py`, `src\levelfx.rs`, which follow the
garage while the screen is up): the light beams over the platforms and the glow
over the chosen class with their own game materials (`blubeams_MATINST`, a
`LightVolume_Base_MAT`, its light fading down the beam and its dust drifting;
`ccGlow_MAT_INST`), the lens flares (the orange soft glows on the wall lamps, the
blue ones at the lamps over the platforms) and the steam rising by those lamps
(`Steam_Directional_FX`): `levels\garage_fx.txt` and `shaders\levelfx\garage.wgsl`,
written by `build_garage.py`. (Without them the beams and glows get the demo's own
stand-in, `assets\shaders\garage_fx.wgsl`.)

The robots are lit as the game lights its preview characters. Most of their light
is the four lamps over the platforms, which light the characters and nothing else
(brightness 6, 10 m radius: `LAC_USER_SELECTED`, neither BSP nor Static), with the
room's lights around them. The game's lights fade with their radius, Bevy's with
the square of the distance, so each light that reaches a robot is made as strong
at the middle of the nearest robot as the game's is there
(`lighting::spawn_light_at`; the maps match theirs a quarter of the way out).
Everything else in the garage takes its light from its light maps, as the game's
static lighting has it, and is drawn with its game material the way the game's
base pass draws it (`SurfaceMaterial` in `src\levelfx.rs`, the material code
compiled from the game's like the effects'): its light map keeps its light from
three directions (the `*_dir.png` pages), so its normal map shades it, its
highlights shine where the light comes from, and it reflects the garage's cube map
as far as its material says. Meshes placed mirrored keep their faces the right way
out. The barrel in front of the platforms is a physics prop, which the game lights
where it stands rather than with a light map, so `build_garage.py` lights it from
the garage's lights (as vertex lighting) and it is drawn with its game material
too. The garage's height fogs fill the pit under the platforms, seen from above as
a teal haze (`assets\shaders\garage_fog.wgsl`, each fog filling what is below its
top, as the game draws them). The lens flares are drawn over the picture as the
game draws them, at the size its flares are in footage of its Customize screen (as
wide as the screen at size 1, wherever they are seen from), so the wall lamps and
whatever stands near them glow orange and the lamps over the platforms cyan. (The
maps' flares are drawn at their lights, sized by the screen's height.) The garage
camera shows the picture through the game's own tone curve (its filmic S-curve,
`lighting::game_filmic`) at the game's display gamma (2.2 at Settings > Display >
Brightness's default; the slider moves it), which footage of the game's garage matches; the 2012 press screenshot of
the line-up is brighter than the game (it was edited). The ambience is the garage Kismet's
(its laboratory bed, the computer screens and the now-and-then beeps, servos and
whispers), from `build_sounds.py`.

The screen opens behind the menu's black and shows only once everything in it can
be drawn: the garage and its light maps loaded, the classes' models with their
textures, the four robots dressed, and then no pipeline (the compiled shaders the
graphics card draws with) left in the making for three frames running
(`open_customize`: the render world's count, `PipelinesPending`), a moment more
after that; it gives up waiting after 12 s. Meanwhile the garage camera draws
everything, robots and cars too, without showing it (its output skipped), so
their pipelines are made then. Before, it showed once the files had loaded, and
the pipelines were only asked for then: for a second the garage was untextured
light blue and the robots black shapes, coming in a piece at a time. Bevy also
holds up the whole frame the first time a camera needs a new blit pipeline (what
copies its picture onto the window or an image), behind every other pipeline
still waiting; the screen's text camera, which draws into an image, needed one,
and opened while the demo was still making its first pipelines that was the
freeze of several seconds. Those are made as the demo starts now
(`warm_blit_pipelines`). The HUD's camera only draws in a match (`show_hud` in
`src\hud.rs`): it drew under the menus, and its "textField" placeholders and glows
showed through as the screen opened.

`src\flash.rs` draws the movie the way Scaleform draws its 3D clips: each image and
line of text is a flat quad placed by its clip's `_x`, `_y`, `_z`, `_xrotation`,
`_yrotation` and scale, seen through Scaleform's 55-degree perspective, with
Flash's colour transforms (`assets\shaders\flash.wgsl`); text is laid out by
Bevy's UI into a texture and cut from it, lined up in its field as the field is
aligned (left, centred or right: the lock's level beside a locked row ends just short
of the lock, as in the game), and a text field's glow filter (or a
drop shadow at no distance) is blurred from the letters' outline in the shader. `src\customize.rs` holds the screen's
state, input, garage, robots and camera, and `src\customize_ui.rs` lays out the
movie's clips with the positions, sizes, colours and timings of its scripts:
the class select's fly-in and the tags' resizing, the slots folding out, the
editor's page stack (pages 2000 and 2800 twips behind each other, turning in 0.4
s), the rows popping out with their white ghosts, the lists and colour grid, the
info panels, and the hint bar laid out as the PC version's. On PC the game shows
its hints as clickable buttons, and so does the demo.

Looking through a weapon or its upgrades in a class's Loadouts, the game gives
the preview character that weapon there and then (`CharacterData.previewWeapon`,
`InstantlyGiveWeapon`); the demo shows the gun on the preview robot's right arm, his
forearm put away for it as in play (from `robot.glb`, the same meshes and clips the
game uses for Jazz). With the gun the game puts the preview character in the
shoot-from-the-hip stance (`GiveWeaponNextFrame` pushes `STANCE_GunArmShootFromHip`),
and the robot anim tree's idle (`TR_Shared_ANIMTREE_p.Robot_ANIMTREE`'s `IdleStance`)
turns from the customization pose (`Cust_Idle`, `STANCE_Customization`'s) to that
stance's `Nav_Idle_Shoot`, the class's own (the Scout's `Shared_SCOUT_ROBO_ANIM`; the
Destroyer's, `Shared_Truck_ROBO_ANIM`'s, a heavier stance): standing ready, the gun
held forward. The demo does the same, blending over the node's quarter of a second
each way, with the gun's idle motion added on top; the gun is painted in the build's
colours. Every class's weapons are shown: the Infiltrator's Scatter Blaster, Path
Blaster, Subsonic Repeater and Nucleon Charge Rifle, the Destroyer's Riot Cannon, A-4
Pulsar Cannon, Thermo Rocket Cannon and Chaos-Rift Combustor, the Titan's X18
Scrapmaker, Energon Harvester, E.D.K. TechVolt and Corrosive Slime Cannon, and the
Scientist's Neutron Assault Rifle, Photon Burst Rifle, Orbital Pistol and Gear
Shredder (the Scientist stands as the Scout does: his animation sets are the
Scout's).

Each class's tag shows its level and experience as the movie's `classDisplay_mc`
script does (`GetLevelForSpecialty`, `GetXpEarnedForSpecialty`, `GetXpNeededForLevel`:
see *Experience and class levels* above): its level on the hexagon, the XP bar from
this level's start to the next's (the `StatBar`'s bar as wide as the experience says),
the marker at its end, and "1350 XP TO 2" over it (MAX at the top level, the bar full
and no marker); the editor's header shows the shards. A slot the class's level hasn't
opened is drawn disabled, "Unlocks at Level 5" (`refreshBuilds`), brightening while
selected (`disabledHover_mc`), and doesn't open. The lists lock what the level hasn't
opened, the lock showing the level it needs (`GetOptionLockStatus`, the row's
`RequiresLevel`); choosing a locked row makes its lock blink, as the game's does. A
new build starts with its class's multiplayer loadout
(`TR_MPPlayerCharacterData_p`'s `<class>_PCD_MP`), and Randomize does what the game's
`ResetCharacter` does: a random part of each kind the build can wear, random colours
and metallic.

What's left out: the decals and the "NEW" marks on what a level has just opened.

### The Customize screen's shop

The parts and armor sets cost what the game asks (`TransCustomization.ini`'s
`RequiresCurrency`, in the shards a level brings): each class's first two armor
sets are free (the Infiltrator's Bumblebee and Breakdown, the Destroyer's Swindle and
Optimus Prime, the Titan's Megatron and Warpath, the Scientist's Jetfire and
Starscream), and the others' parts cost 600 for the arms, 1,000 for the shoulders,
1,500 for the chest, 1,000 for the head, 800 for the legs and 500 for the wheels,
wings or cannon: 5,400 for a whole set. The DLC's sets, which the game sells
separately, cost the same here (4,900 for those whose wheels, wings or cannon are a
free set's). A part bought is bought for every build and every class that can wear
it. As in the game (the list's `configArmorSetButton`, `saveAndClose` and
`purchaseArmorSet`, `TnXpManager.GetOptionLockStatus`):

- A part not bought shows its price left of its row (`price_mc`, in the game's
  Energon Numbers Wide: the shard sign and the number), sliding in with the locks as
  the list opens; an armor set shows what its parts not yet bought cost together.
  You can try a part or set on before buying it: the robot wears it while the list is
  on it, and going back takes it off.
- Choosing a part or set you can afford buys it and puts it on: the shards come off
  the header's count, the game's **Acquired** notice comes up in the middle of the
  screen ("Head : Rogue" and the price, `mc_notifyChallenge`, for three seconds) with
  its sound. Choosing an armor set you already own puts it on (with the game's
  locked-item blip, which its script plays for each part already worn).
- Without enough shards nothing is bought and the list stays open: the header's
  shard sign swells, turns orange and blinks (`showInsufficientFunds`), with the
  game's error sound.
- The bonus heads that need more than shards show locked with what they need, as the
  game's do: **25** (the class at level 25, or Prime Mode), **PRIME**, **PRIME 25**
  and **PRIME 100** (Prime Mode with every class at 25).
- New builds, and Randomize, only use parts you can wear. A saved build wearing a
  part you can't (not bought, or locked) loses it for a random one of its kind you
  can, and is saved that way, as the game does when DLC goes missing
  (`ResetPartsThatAreNoLongerAvailable`).
- The decal sets cost 400 shards each, for every class (see *The decal sets*
  below).

The prices and locks come from `build_customize.py` (`--parts` rewrites them);
`build_menu.py` adds the price font (`$NumberFontWide`) and `build_sounds.py` the
shop's two sounds (`UI_MONEY_DEPOSIT_SUCESS`, `UI_MONEY_DEPOSIT_FAIL`). The shards
come with the levels only: `progress.txt` is signed, so they can't be set by hand
(see *The progress file* below). Nor can the prices and locks: `customize.txt` is
built into the program, and the copy in `assets` isn't read.

### The decal sets

The Armor page's **Decals** list holds the game's eight decal sets beside None:
Targetmasters, Lightning Strikeforce, Warchargers, Astro Squad, Protectobots, Energo
Squad, Battle Squad and Lazer Rods, every class's, sold and worn like the parts
(above). A set is eight decals, each a small quad cut from one 1024 x 1024 sheet of
emblems, stripes and chevrons, hung as the game hangs them (`TnMeshPartDecals`: each
quad on every socket whose name starts with its own, `DecalBack01`, `DecalFront02`
and so on): on the robot's chest and all over his vehicle, mirrored on its right
side, so every chest and vehicle wears a set its own way. They glow in his faction's
energon colour (red for an Autobot, purple for a Decepticon) with the energon's
flashes running through them, as the game draws them (`TR_Decals1_Mat_INST`, an
instance of its faction logos' `TR_FactionLogo_Mat`), and go with him: they fade
with your own robot as the camera comes close and vanish as the Cloak's overlay
reaches them and as a body dissolves. The other players see your set.

`build_decals.py` writes the quads, the sheet and the sets (`assets\models\decals`)
from `UI_Lobby_m.xxx`, and `build_class.py --decals` adds each vehicle's decal
sockets to its class's file (the robots' are among its parts' sockets already):

```
python build_decals.py --umodel "C:\...\TransGame\CookedPC" decals
python build_decals.py decals "C:\...\TransGame\CookedPC" ..\assets
python build_class.py --decals infiltrator "C:\...\TransGame\CookedPC" "C:\...\TransGame" ..\assets
```

(the first prints the two umodel commands that export the quads and the sheet into
`decals`; run `--decals` for each class: infiltrator, destroyer, titan, scientist).

### The progress file

`progress.txt`, next to the `assets` folder (as `builds.txt` and `settings.txt`),
keeps each class's experience, the shards, the parts bought and Prime Mode
(`src\xp.rs`). The game keeps these in the player's profile, out of his reach; the
demo signs its file, so that changing it by hand gets nothing:

- **The signature** is its last line, `sig` and 16 hex digits: a keyed hash of the
  values above it (SipHash-2-4, under a 128-bit key built into the demo,
  `SIGNING_KEY` in `src\xp.rs`), written each time the demo saves the file. Only the
  values are signed: a file opened and saved again unchanged, even with its line ends
  changed by the editor, is still the demo's.
- **Changed by hand**, its values no longer the ones signed (or its signature taken
  off), the file isn't taken. As the demo starts it is set aside as
  `progress.rejected.txt` (`progress.rejected.2.txt` and so on, should that be there
  already) and the progress starts over from nothing, as the game's does with a
  damaged profile: the main menu opens on the game's notice for it, **Error**,
  "Profile data is corrupt." with Ok (`TnPlayerController.OnReadProfileSettingsComplete`:
  `MessagePrompts.GenericErrorTitle` and `ResetCorruptProfile`, which `build_menu.py`
  adds to `menu.txt`). Nothing is deleted: put back as it was, a file set aside can
  be renamed `progress.txt` again and is taken.
- **From an earlier version of the demo**, unsigned, the file is taken as it is and
  signed straight away, so the experience earned before is kept. (It is known by the
  first line those versions wrote, which the program holds only as a hash, and which
  the demo's own first line now is nothing like.)
- **Saved whole**: the demo writes it into a file beside it, then moves that over
  it, so a crash or a power cut while it saves leaves the one before rather than
  half a file (which would no longer match its signature).

It stops changes made in a text editor. It can't stop someone with the demo's source
(or who digs the key out of the program) from signing a file of his own, since the
program has to hold the key to check the signature; that is so for any game that
keeps its saves on the player's own PC. Should `SIGNING_KEY` change, every
`progress.txt` signed with the key before is set aside.

What the levels open, and what things cost, can't be changed by hand either: they
come from `customize.txt`, which is built into the program as it is compiled rather
than read from the `assets` folder (see *What's new*).

### The characters' voices

Each build has a voice (Customize's Select Voice): one of its class's three
(`TnDataProvider_RobotVoice`: Voice 1, 2 and 3), or None, the demo's own, which
keeps the character silent. Every class's Voice *n* is the same pair of the game's
`HmSoundEventSet`s, the Autobots' `SoundEvents.Multiplayer.CHR_MP_DX_AUTOBOT_0n` and the
Decepticons' `CHR_MP_DX_DECEPTICON_0n` (`TransCustomization.ini`'s `ObjectPath` and
`DecepticonObjectPath`), and a character speaks with his side's (the Decepticons' on
Downfall and Oblivion). `src\voice.rs` plays them as the game's players' dialog does:

* the events a set answers, each with a cue of recorded lines (one picked at random):
  a reload beginning and ending, picking up a weapon, ammo or health, a kill (a
  headshot, a melee blow and the Gear Shredder have lines of their own; otherwise the
  weapon's damage type's `DialogDamageCategory` picks the gun's, the rocket's or the
  explosive's, as `TnPlayerController.PlayTargetKilledDialog` does: the Riot Cannon
  and the E.D.K. TechVolt have none), firing a grenade launcher (the A-4 Pulsar
  Cannon, the Chaos-Rift Combustor: `TnWeaponGrenadeLauncherBase`), being hit (a
  grunt), his health falling below a tenth (`TnPlayerPawn.UpdateHealthDialog`), his
  death (a cry, `TnPawn.RobotDying`), a melee swing (a grunt) and a teammate's death;
* whether he says it: each event's dialog group (`HmSoundEventDialogGroup`) keeps
  quiet for a while after it has spoken (`ReTriggerDelayMin` to `Max`: 25 to 65
  seconds after a kill line, 15 to 55 after a reload line, 3 to 6 after a grunt...),
  and so does each group above it (the combat lines' group keeps them a second or two
  apart); as he comes into the match each group starts that quiet unless a roll of
  its `InitialChanceToPlay` says otherwise (`GetInitialDelay`: a third of the time
  for the reload and pick-up lines); and he says one line at a time.

The other players hear his lines from his Jazz, sent with his other sounds. On the
Customize screen, moving to a voice in the list has the preview robot say that
voice's gun-kill line (`TnCharacterScriptBinding`'s `PreviewSound`), on the side
its colours show. `build_sounds.py` writes the sets, their cues and the groups' rules
to `assets\sounds\voices.txt`. The grunts and death cries come with
`MP_Array_BASE_m_LOC_int.xxx` (85 recordings); the spoken lines (600 recordings: five
takes of each line, one picked at random, for each of the six sets) are streamed from
the voices' own packages, `WL_DX_MPAUT1` to `3` and `WL_DX_MPDEC1` to `3` in
`CookedPC` (about 3 MB each), which `build_sounds.py` reads when they are there.

### The Infiltrator's characters

Every Infiltrator armor set the game offers in multiplayer is in the demo, with its
vehicle, its transformation and its own animations: Bumblebee (the game's Recon
parts), Breakdown (Saboteur), Sideswipe (Rogue), Deadend (Assassin), Jazz (Ghost),
Dragstrip (Slayer) and the DLC's Wheeljack (Scrapper) and Snarl (Roadside), with
the bonus heads (Eliminator, Predator, Fire Breather, Creeper and Elite) and the
extra wheels (Striker, Throwback, Convoy and Gladiator). On the Customize screen's
Armor page any part goes with any other, as in the game: the head, chest (which
decides the vehicle), shoulders, arms, legs and wheels each take one of the parts,
and an armor set puts on all of its own. The preview robots wear the build in its
colours (the Autobot ones, or the Decepticon ones with Toggle Faction; the Metallic
slider turns the paint into coloured metal that reflects the garage, as the game's
does: see the shading below) with their energon glowing in the faction's colour, red
for the Autobots and purple for the Decepticons, as on the game's Customize screen,
whatever Settings > Display > Team Coloring Mode says (the game's
`TnCharacterApplier.GetEnergonColorForTeam` gives the player's own team's cyan on
Ally vs Enemy only in a team game or a lobby). Change Form plays the chest's vehicle's transformation. In a match Jazz is the chosen build, in its Decepticon colours on
Downfall and Oblivion (the Decepticons' maps), and the others in the game see him
as he is on his PC: each player's build goes with his Jazz over the network. Every
PC needs this version of the demo: an older one is turned away with the game's
"incompatible version" message.

How the game builds a character, which the demo follows (`tools\class_data.py`
reads it from the game's data):

- Every part is a mesh on the one robot skeleton that `robot.glb` already has with
  all of Jazz's moves, so `src\character.rs` skins each part to robot.glb's own
  bones, and every character moves as Jazz does. The wheels hang on the parts'
  wheel sockets (on the forearms and behind the knees).
- The chest decides the vehicle: its model, its clips and the robot's half of the
  transformation. Each transformation is timed by its own clips (when the robot
  vanishes into the car and the car appears, and the other way round: their
  ToggleHidden notifies), the jets come from where the vehicle's blueprint puts its
  effects (its hover jets, jump jets and boosters), and the wheels turn by their own
  size. A car is a single rigid body in the game (its physics asset's one body is on
  the skeleton's root bone), so its own clips can't move that bone, and the demo
  holds it too; each transformation is moved as a whole so that the car is just
  where it drives and hovers where the two meet.
  While the robot and the car both show, the car is drawn where the robot's pieces
  are. The game hangs each form's mesh on the player by its own offset (the robot's
  under the middle of his collision cylinder, the vehicle's under the middle of its
  bounds) and moves the player between the two, so where the demo drew both from
  the feet the car came out below the robot it was made of (Bumblebee's more than a
  metre down) and went below the one unfolding out of it. Folding up, the car now
  comes out of the robot up where he has leapt and settles to where it hovers;
  unfolding, the robot comes out of the car where it is and both are carried to
  where he stands over half a second, as the game carries the meshes
  (`TnPawn.MoveToSafeTransformationLocation`), in matches and in the Customize
  preview. How high is measured from each vehicle's clips (`tools\transform_fit.py`:
  the class's `vtransform` lines). The half seen as it starts blends in from the
  pose it was in over the game's 0.115 s (`TnTransformation`'s blend-in time): the
  robot's folding up from whatever he was doing, the car's unfolding from driving or
  hovering. The robot's half keeps the turn
  its clip puts on the skeleton's root (`C_Root_Reference_XR`): the Rogue set's
  (Sideswipe's `Transform_ToVehicle_ROBO`, in `SideSwipe_ROBO_ANIM_NEW`) is made with
  the whole robot turned 122.6 degrees on it and the rest of the skeleton turned
  back, so without it he flipped off to the side and folded up facing away from
  his car. (No other set's turns its root; the roots' offsets are left out, as
  before.)
- A forearm part brings its forearm pieces' animation as the gun moves to the other
  hand (they shrink away and grow back as its ScaleBone notifies time them), an
  upper arm its shoulder pads flipping as a gun comes out.
- The paint: each texture set's mask (the blue of its `Masks_NormY`) says where the
  primary colour goes (over half) and the secondary (under), multiplying the part's
  own colours (`assets\shaders\paint.wgsl`). The guns are painted the same way, as
  the game paints the weapon in hand (`TnCharacterApplier.ApplyColorsToCharacter`
  passes the colours on to `TnWeapon.ApplyCharacterColorsToMeshes`; the weapons'
  materials take `Cust_Color_A` and `Cust_COLOR_B` as the parts' do): every gun in
  robot.glb and the arm built into them, and the car guns and the trucks' rocket
  launchers. Their texture sets are read straight from `CookedPC` (`all_shared.xxx`,
  with the full-size pictures from each weapon's `WEP_*_p.xxx`). (The vehicles'
  weapons don't show on the Customize screen: the game hides a vehicle's weapon until
  it first fires.)
- The shading: the parts are lit as the game's character material lights them
  (`TR_BRDF_CustChar_TEXSET_MAT`, `ENV_BRDF_A_MAT` underneath, as the game's compiled
  shaders for it have it), in `assets\shaders\paint.wgsl`. A texture set's gloss mask
  (the green of its `Masks_NormY`) makes it glossy as far as the part's material says
  (each part's own values: how sharp its highlight gets, where its gloss starts and
  ends, and how bright its colour is lit; `tools\build_paint.py` reads them from the
  game's material instances into `assets\models\paint.txt`). What it reflects is a
  coefficient: about 0.72 for the unpainted metal (its `CoefficientColor_C`), and for
  the paint 0.1 (what the game gives a character's paint in a multiplayer game,
  `TnCharacterApplier.ExtractColors`) that the **Metallic slider** (the game's
  `MetallicOffset`) turns into the paint's own colour, up to three times as bright;
  the colour's own light goes down as much as the reflection's grows. The reflection
  is the cube map of the room the character is in (the garage's on the Customize
  screen, the level's own captured one on Ignition and Ancients, and on Downfall and
  Oblivion, which capture none, the characters' material's default, `rommCubeMap`),
  blurred as far as the gloss is low and lit by the light coming from that way. So at
  0 the paint is coloured enamel on chrome-like metal, and at full Metallic it is
  coloured chrome. The lights' highlights are the game's too (Bevy's lights with the
  game's reflection coefficient and highlight power).
- Snarl walks: what is hovering for the cars is walking for him, and boosting is
  running, on his legs and with no jets. His clips were made for the full-size
  Dinobot: his multiplayer mesh plays them at 0.6 of their reach (its
  TranslationScale), and so does the demo, which keeps his pieces together. He
  jumps in his own jump clip (looping until he comes down) and lands in his landing
  clip after half a second in the air, the hard one on the move and the idle one
  standing still (his blueprint's `HoverJumpAnim`, `HoverLandAnim` and
  `HoverIdleLandAnim`, which Slug and Grimlock play too).
- The sounds: a vehicle sounds as its form's sound event set says (`VEH_SPORT_01`
  for Jazz's and Sideswipe's cars, `VEH_SPORT_02` for Bumblebee's, Dragstrip's and
  Wheeljack's, `VEH_MUSCLE_01` for Breakdown's and Deadend's, `VEH_DINOBOT` for
  Snarl): its engine, boost on and off, tyre squeal, crashes, jumps and landings,
  and its `DeathSound` when it is destroyed. The engines are mixed by speed from
  their own cues' loops the way Jazz's car's is, each as loud against his as in the
  game at rest and flat out (`ENGINES` in `src\main.rs`). Where Snarl's set has no
  cue he jumps with his one jump, crashes with his heavy crash, and his feet don't
  squeal; his own clips' sound notifies stomp his feet as he walks and runs, and
  swish his tail and growl as he stands (`BL_VEH_TANK.SNARL_FS_DEFAULT_WALK`,
  `SNARL_VOX_IDLE`...). A transformation plays what its clips' sound notifies play
  (the robot's and the vehicle's: `BL_TRANSFORM.BREAKDOWN_BOT2VEH` and the like,
  Bumblebee's through the Infiltrator's sound event set), in a match and when a
  preview robot changes form on the Customize screen.

The files, made by `tools\build_class.py` (each class's in its own folder: the
Infiltrator's in `assets\models\infiltrator`, the Destroyer's in
`assets\models\destroyer`, the Titan's in `assets\models\titan`, the Scientist's in
`assets\models\scientist`):

```
assets\models\<class>\parts.glb          every part's meshes, and the wheels
assets\models\<class>\clips.glb          the robot's clips the parts bring (the transformations, forearms, shoulders)
assets\models\<class>\clips_mirror.glb   the same, mirrored for the gun in the left hand
assets\models\<class>\robot_clips.glb    the class's own moves (the Infiltrator's only its sword swing: robot.glb's are his)
assets\models\<class>\robot_clips_mirror.glb   the same, mirrored
assets\models\<class>\vehicle_*.glb      each vehicle with its guns and clips
assets\models\<class>\textures\*.ktx2    the texture sets, with the paint mask, and their glow (*_e: the
                                         parts' own light in red, their energon in green)
assets\models\<class>\<class>.txt        the class (its size, moves, camera and sounds), its parts, sets,
                                         sockets, vehicles (their handling, timings and sounds)
```

Without the Infiltrator's Jazz wears robot.glb's own parts and drives `vehicle.glb`,
as before; without a class's own, it can't be played and its preview robot stands
in with the Infiltrator's. To make them, export the meshes, animation sets and
textures with umodel (the script prints the commands, object lists and all: run it
once for the meshes and animation sets, and again once they are exported for their
textures), then build (here the Destroyer's; `build_infiltrator.py` is the same for
the Infiltrator):

```
cd Documents\foc_esc\tools
python build_class.py --umodel destroyer "C:\...\TransGame\CookedPC" "C:\...\Transformers Fall of Cybertron\TransGame" C:\dstx
python build_class.py destroyer C:\dstx "C:\...\TransGame\CookedPC" "C:\...\Transformers Fall of Cybertron\TransGame" ..\assets
```

`build_customize.py` (above) lists every class's parts and armor sets in
`customize.txt`, and `build_sounds.py` takes the sounds the classes' files name.

The glow textures are made with the rest. To give a class already built its glow
without building it again (straight from the game's packages, no exports needed),
run the glow pass, which writes its glow textures and points its models' materials
at them:

```
python build_class.py --glow destroyer "C:\...\TransGame\CookedPC" ..\assets
```

The parts pass does the same for its parts (from the exports, as the build): its
`parts.glb` and the textures it uses, and its file's part lines (the Scientist's
robot wings came this way; run the glow pass after it):

```
python build_class.py --parts scientist C:\scix "C:\...\TransGame\CookedPC" "C:\...\Transformers Fall of Cybertron\TransGame" ..\assets
```

The interiors pass gives a class already built its vehicles' insides, the
undersides of the panels that show as they change form, straight from the game's
packages: each vehicle model's material drawn as dark glass for want of a texture
set is made again for the mesh that uses it. `VH_Interior_MAT` is a name twelve of
the game's materials share (Swoop's own material and eleven vehicles' instances of
`Interior_DIF__TEXSET`), so each mesh now takes the one its own data points at
(`class_data.py`'s `mesh_material`); the insides made of pictures (`Interior_DIF`,
the Breakdown's and Sideswipe's) and the plain materials (the energon-lit
`InteriorAlt_Energon_MAT` of Soundwave, Onslaught and Deadend, the Swarmer's inside
of Snarl and Swoop, Vortex's `TR_Base_Mat`) are read from CookedPC. A full build of a
class does the same:

```
python build_class.py --interiors titan "C:\...\TransGame\CookedPC" ..\assets
```

The glow masks are each texture set's `_Masks_NormY` red, kept one of two ways:
resting at half (the game's two-channel red: darker is the part's own light, as
Jazz's visor, brighter its energon, as his eyes) or resting at nothing (brighter is
the light, own and energon alike, as Breakdown's wheel spokes). The game's
character material lights them (`TR_Emissive_MF`, `TR_Energon_MF`), and
`assets\shaders\paint.wgsl` does the same: the painted colour times 3.75 for the
own light, the faction's energon colour times 10 for the energon, pulsing.

The parts' material values (`assets\models\paint.txt`: the shading above) come
straight from the game's packages too, every class's, vehicle's, gun's and enemy's at
once:

```
python build_paint.py "C:\...\TransGame\CookedPC" ..\assets
```

What's left out: the characters' own faction logos (their `TR_FactionLogo_Mat_INST`
sections), the few car interiors whose texture sets aren't cooked into the
multiplayer maps (drawn as dark glass), and of the vehicles' sounds the boost's held
loop and the other players' engines (only your own is heard, as before). The DLC
sets are unlocked for everyone. The cars drive by Escalation's handling
(`Car_Physics`, `HoverCar_Physics`; Snarl by his own, `Snarl_Physics` and
`HoverSnarl_Physics`).

### The Destroyer's characters

Every Destroyer armor set the game offers in multiplayer is in the demo too, with
its truck, its transformation and its own animations: Swindle (the game's Harbinger
parts), Optimus Prime (Striker), Ratchet (Mercenary), Ironhide (Commando),
Soundwave (Warmonger), Onslaught (Road Brawler) and the DLC's Zeta Prime (Battle
Seeker), G1 Optimus (Throwback), Ultra Magnus (Convoy), Hound (Gladiator) and Slug
(Stampede), with the bonus heads (Rampage, Omega, Parasite and Elite) and wheels.
They are built and painted just as the Infiltrator's are (above), on the same
skeleton; what makes a Destroyer a Destroyer comes from his class's robot
definition (`CustomLeader_ROBODEF`) and its sets:

- **His size:** his parts are drawn at 0.8 of their size (its `RobotMeshMergeScale`;
  the Infiltrator's 0.6), and his collision is 5 m tall and 2 m round (its
  `CollisionHeight` 250 and `CollisionRadius` 200; the Infiltrator's 3.1 m and 1.4 m):
  he bumps into walls and reaches the stations at that size, his shots and the
  HUD's markers start from the middle of that body, and the camera fades him as it
  comes near it.
- **His moves:** the class's own animation sets come after the shared ones, and as
  the game looks a clip up (the last set that has it) a Destroyer jogs, sprints,
  dashes (on the ground and in the air), takes off for a jump, shoots on the move
  and walks in fine aim with the heavy versions in `Shared_Truck_ROBO_ANIM`, turns
  on the spot in fine aim with its pivots, and swings his own sword (his melee set's
  attack, `Destroyer_ROBO_ANIM`'s `Melee_Sword_Destroyer`, with the sword's swing
  and the Ion Blade: see "The multiplayer melee"). His footsteps keep time with those clips (each class's stride is
  worked out from its clips against Jazz's), and on the Customize screen he holds a
  gun in his class's own stance. Every other move (the idle, landings, hits,
  deaths, reloading and the gun's own animations) is the shared one, as in the game.
- **His sounds:** his footsteps, jumps and landings are his class's sound event set's
  (`CHR_MP_DESTROYER_1`: `BL_FS_LRG_BOT`, the large robots' heavy steps; the
  Infiltrator's are the small robots', `BL_FS_SML_BOT`), and he falls with the heavy
  body fall when he dies.
- **His camera:** on foot the class's camera set's (`Truck_MP_CAMSET`),
  `Truck_OverTheShoulder_STRATEGY`, 8.75 m out (Jazz's 5.5), aimed 2.85 m to the
  side and 2 to 2.5 m over the middle of his body, further ahead as the view comes
  up (its offsets by pitch); the Nucleon Charge Rifle's scope takes it in 3.75 m
  behind him and 3.5 m up. In his truck, Escalation's: the camera set of the
  Escalation character who drives that truck (Optimus Prime's, Ratchet's and
  Soundwave's `Truck_Optimus_CAMSET`, Swindle's and Onslaught's `Truck_ALT_CAMSET`),
  or for a truck no Escalation character drives `Truck_Optimus_CAMSET`. Optimus
  Prime's: driving, `Truck_Optimus_STRATEGY` follows 9 m behind, anchored 2.1 m over
  the truck's middle (where the game's pawn is: 1.45 m up), following it round and
  blurring as it boosts, as the cars' does; hovering, `HoverTruck_Optimus_STRATEGY`
  7 m out (8.5 m strafing), aimed 2.1 m over its middle less 0.26 m with the view
  level (0.35 m in the middle of its pitch range), the view kept within 25 degrees
  of level, the
  mouse turning it no faster than the hovering car's (328 degrees a second across
  at the middle Camera Sensitivity). G1 Optimus's truck and Slug keep the
  multiplayer set's (`Truck_MP_CAMSET`'s camera states made for them,
  `TnPCS_G1Optimus` and `TnPCS_Slug`: closer for the one, further out and higher for
  the other).
- **His trucks:** each drives by the blueprints Escalation gives its form
  (`Truck_Physics`, `HoverTruck_Physics`; the jeeps' own, Swindle's and Hound's) as
  the cars do, steered with the mouse on its wheels, with its own push, tyres,
  steering lock and wheelbase: 34 m/s at most on its wheels (Jazz's car 31), slower
  to get going (MaxAcceleration 1000, the car's 1200), 15 m/s hovering (the jeeps
  22), with its own jumps (further ahead), its own hover
  height, and its bulk against walls taken from its model. Boosting, each burns its
  flames from its own boosters (its form's boost effects' sockets, the afterburners
  of Ratchet's, Ironhide's, Soundwave's and Onslaught's among them). Slug walks and
  runs as Snarl does, with his own footsteps, tail and growls. Soundwave's truck's
  wheels don't turn, as in the game (its form gives it none to turn).
- **His rockets:** in his truck, the left mouse button fires the class's vehicle
  weapon (its character data's `VehicleWeapons`), the game's truck rockets
  (`TnWeaponRocketVehicle`, `Rocket_Vehicle_SV_WEPDATA`), from the two rocket
  launchers on the truck's weapon sockets (`WEP_RocketLauncherVH_SKEL`, painted in the
  build's colours, in the car guns' place: they ride up out of the truck on its own
  gun pods as it fires, `ADD_Weapon_Equip_VEH`, hidden until it first fires and
  folding away 3 s after the last shot or as they reload, as the car guns do). Each
  pull of the trigger fires a burst of two, one from each launcher, 0.225 s apart, and
  the next burst waits a second (its `BurstDelay`) from 0.225 s after the last rocket.
  It holds 8 (8 more in reserve) and reloads by itself once they are gone, in 1.5 s;
  its spread is 0.02 to 0.06, opening by 0.04 a rocket and settling back over a
  second. Each rocket (`RocketVh_SP_PROJDATA`, its Escalation data: the projectile's
  `Data`, as `TnProjectile.GetProjectileData` picks it) flies straight at 120 m/s for
  up to 4 s and goes off on the first thing in its way: 55 damage to every enemy
  within 15 m, in full out to 7.5 m, less and less to a quarter of it at 14 m and a
  quarter beyond (its `RadiusDamageScale`), and a quarter of that to the Destroyer
  himself if he is that close. Each rocket jolts the camera (its `CameraShakes`; its
  `CameraRecoils` neither pitch nor narrow the view). Its sounds (`BL_WPN_GUN_ROCKET`:
  the shot, the reload, the rockets' flight loop and their blast,
  `VEH_EXPL_IMPT_WORLD`; the ballistic dry fire when it is empty) and effects (the
  muzzle flash `MuzzleFlash_Blaster_FX` on each launcher's muzzle, the rocket
  `Tracer_HomRocketVH_FX` and its blast `Squib_HomRocket_01_FX` at three quarters of
  its size) are the game's, and the HUD shows the game's rocket crosshair
  (`mc_crosshairRocket`, its prongs pushed out by the spread). Online, the rockets fly
  and go off on the other players' PCs too.
- **Their sounds:** each truck's form's sound event set: `VEH_JEEP_01` (Swindle's and
  Hound's jeeps), `VEH_TRUCK_01` (Optimus Prime's, G1 Optimus's and Ultra Magnus's),
  `VEH_TRUCK_02` (the others') and `VEH_DINOBOT` (Slug): the engine, boost, tyres
  (the jeeps skid on dirt), crashes, jumps, landings and death, and each
  transformation's own sounds (`BL_TRANSFORM.OPTIMUS_BOT2VEH`...). A truck's engine
  cues play the same loops at rest and flat out, louder under load, so the demo
  plays them that way, each as loud against Jazz's car as in the game (the jeeps a
  good deal louder, the big trucks a little quieter).

What's left out: the Destroyer's abilities (he has none; he throws the demo's grenade
as Jazz does: his weapons are his own, *The Destroyer's weapons*, above), his trucks' ram, the launchers' own firing animation (`Weapon_Fire`, the
rocket in the tube shrinking away as it leaves and growing back: the launchers are
held as it starts), the blast's embers (an emitter `tools\ue3\particles.py` can't
read yet) and the class's own health and armor (he has Jazz's). His files add about
225 MB to `assets\models` (most of it the texture sets).

### The Titan's characters

Every Titan armor set the game offers in multiplayer is in the demo as well, with its
tank, its transformation and its own animations: Megatron (the game's Gunner parts),
Warpath (Obliterator), Brawl (Tanker), Scattershot (Wrecker), Demolisher (Bruiser),
Quake (Steel Crusher) and the DLC's Perceptor (Annihilator), Hardshell (Beetle) and
Grimlock (Berserker), with the bonus heads (Beast, Warlord, Crawler and Elite). A
Titan has no wheels: his sixth part is his **cannon** (the Armor page's Cannon row,
in the wheels' place), which he carries on his back as a robot (the chests' cannon
sockets) and which sits on his tank as its gun (the tanks' own cannon sockets: two
on Brawl's, four on Demolisher's), as in the game; Grimlock's chest takes none. They
are built and painted as the other classes' are (above); from the class's robot
definition (`CustomSoldier_ROBODEF`) and its sets:

- **His size:** his parts are drawn at 0.9 of their size and his collision is 5.4 m
  tall and 1.75 m round (its `CollisionHeight` 270 and `CollisionRadius` 175), with
  everything that follows from it, as for the Destroyer.
- **His moves and sounds:** the heavy moves of `Shared_Truck_ROBO_ANIM`, as the
  Destroyer's, and his own sword swing (`Titan_ROBO_ANIM`'s `Melee_Sword_Titan`); his
  class's sound event set (`CHR_MP_TITAN_1`) gives him the large robots' heavy steps.
- **His camera:** on foot the class's camera set's (`MP_Tank_CAMSET`), the
  Destroyer's (`Truck_OverTheShoulder_STRATEGY`); in his tank Escalation's (Warpath's
  and Megatron's `Tank_Megatron_CAMSET`, Brawl's, Quake's and Perceptor's
  `Tank_SP_ALT_CAMSET`, `Tank_Megatron_CAMSET` for the others): Warpath's
  `Tank_Megatron_STRATEGY` 8.25 m behind and 2.1 m over the tank's middle (the
  middle of its bounds, where the game's pawn is) with the view level (its 7.25 m
  orbit and its aim's offsets, a metre ahead of it), 10 m
  while boosting, 75 degrees wide; the mouse turning the view as
  the game's camera turns a vehicle's under the mouse: as fast as the mouse goes,
  but no faster than 295 degrees a second across and 98 up and down at the middle
  Camera Sensitivity (its 45 and 15 at an input of 1.5, times 3.5 with its ramp),
  197 and 66 while it boosts (the input held within 1:
  `TnPlayerController.AdjustStickInputForVehicleUnderMouseControl`). Grimlock has his own
  (`TnPCS_Grimlock`), 9.5 m out and 3.65 m up (10 m boosting). The view looks no
  more than 30 degrees down or up from the tank's own pitch (the strategy's
  `PitchRange`, `PitchRelativeToTargetActor`: `TANK_PITCH_RANGE` in `src\main.rs`), so
  the cannon, which fires where you look, can't be aimed at the ground at the tank's
  feet (it could look 69 degrees down, as the demo's other vehicles without their own
  range, and fire into its own blast).
- **His tanks:** a tank only hovers, as the game's do (`TnTankForm` and
  `TnHoverTankSimulation`, by the blueprint Escalation gives the forms,
  `HoverTank_Physics`; Grimlock's his own). Its hull always faces
  where you look, and it strafes in any direction (W A S D) at 17 m/s. **Boost** (the
  right mouse button) and it flies straight ahead at 22.5 m/s (Grimlock 20); when the
  boost ends it drifts for 0.15 s before its grip comes back, and it can't boost again
  until then. It speeds up at 15 m/s² and jumps (Space) at 15.5 m/s with its nose
  kicked up.
  Its jets are the game's particle systems on its own sockets, in its driver's energon
  colour (his faction's in a match, as his glow is: red or purple): hover jets
  underneath for as long as it floats (`hover_tank01_FX`),
  boosters as it boosts (`Afterburner_D_FX`; Grimlock's `Afterburner_A_Grim_FX`), a
  blast as it jumps (`Jump_tank01_FX`) and a burst as it lands after half a second
  in the air (`Vehland_High_Lrg_FX`). How high a tank floats is left to the game's
  suspension physics, which the demo doesn't run, so each floats with its lowest
  point 0.6 m off the ground in its hover pose, which matches the game's videos;
  Hardshell stands on its legs, and Grimlock walks: his hover is walking on his legs
  and his boost running, as Snarl's and Slug's are, strafing and boosting as the
  tanks do. On the Customize screen a tank comes out of its transformation on the
  ground and rises to that height on its hover jets, as the game's garage shows it
  (cyan there too, as the game's default team colouring has it; with its faction
  colouring the game shows them in the side's colour, red or purple).
- **His cannon:** in his tank, the left mouse button fires the class's vehicle
  weapon, the game's tank cannon (`TnWeaponTankCannon`, `TankCannon_SV_WEPDATA`). The
  cannon turns to where you look (its bone set to the view's pitch over the hull's
  heading, as `TnTankForm` does) and each pull of the trigger fires one shell at the
  crosshair from its muzzle (`WeaponSocket_TankCannon`). The shell (`TankShell_SV_PROJDATA`)
  flies at 170 m/s for up to 4 s and goes off on the first thing in its way: 300
  damage to every enemy within 12.5 m, in full out to 5 m and less and less to
  nothing at 12.5 m (its `RadiusDamageScale`), and a quarter of it to the Titan himself
  if he is that close, as for the Riot Cannon. It holds one shell (5 more in reserve)
  and loads the next in 2.25 s, with the cannon's recoil and reload animation
  (`ADD_TankShooting_Reload`); its spread is 0.02 to 0.08, opening by 0.06 a shot and
  settling back over 3 s, with the game's cannon crosshair; each shot jolts the camera
  (its `CameraShakes`) and stops a boost, the tank drifting for a moment. Its sounds
  (`BL_WPN_GUN_TANK_CANNON`, in `mp_shared.xxx`: the shot, which is always its
  low-ammo shot as the game plays it, the reload, the shell's flight loop and its
  blast; the plasma dry fire when it is empty) and effects are the game's: the muzzle
  flash on the barrel (`Tank_MuzzleFlash_FX`), the shell flying as
  `Tracer_HomRocket_FX` (at 1.5 times its size: the shell's projectile mesh's
  `DrawScale`, `TnProjectile.UpdateMesh`) and going off as `TankCannon_ImpactExp_FX`. Online, its shells
  fly and go off on the other players' PCs too, and the others see your cannon aim.
- **Their sounds:** each tank's form's sound event set: `VEH_TANK_01` for the tanks
  and `VEH_DINOBOT` for Grimlock (the engine, boost, jumps, landings, crashes and
  death), Grimlock's stomping feet as he walks and runs, and each transformation's own
  sounds (`BL_TRANSFORM.MEGATRON_BOT2VEH`...). The tank engine's cues, like the
  trucks', play the same loops at rest and flat out, louder under load (a little
  louder than Jazz's car at rest, a little quieter flat out, as in the game).

What's left out: the Titan's abilities (he has none; his weapons are his own, *The
Titan's weapons*, above), his tanks' ram, the cannon's icon in the HUD's ammo
notes (they show the number alone), and the class's own health and armor (he has
Jazz's). His files add about 175 MB to
`assets\models` (most of it the texture sets).

### The Scientist's characters

Every Scientist armor set the game offers in multiplayer is in the demo too, with its
jet, its transformation and its own animations: Jetfire (the game's Guardian parts),
Starscream (Arclight), Air Raid (Protector), Vortex (Vanguard), Silverbolt
(Skymaster), Shockwave (Defender) and the DLC's Blastoff (Wingman), Sharpshot (Buzz),
Kickback (Stinger) and Swoop (Jurassic), with the bonus heads (Hunter, Tyrant, Pest
and Elite). Vortex's vehicle is a helicopter, Sharpshot's and Kickback's are
Insecticons and Swoop's a Dinobot jet. A Scientist has no wheels: his sixth part is
his **wings** (the Armor page's Wings row, in the wheels' place), which he wears on
his back as a robot (the chests' wing sockets) and which his jet flies on (the jets'
own wing sockets), as in the game; Vortex's, Sharpshot's, Kickback's and Swoop's
chests take none. Each set's wings are two meshes, as the game's are: the robot's
(its `RB_*_PART_Wing`: `RB_Airraid_STAT_Wing` and the like, in the robot's own
texture sets, `RB_AirRaid_Wing_TEXSET`...), which stand upright behind his shoulders,
and the jet's (`VH_*_PART_Wing`, which the robot doesn't wear: `VH_Airraid_STAT_Wing`,
its wing). The jet's flat wings on the robot's sockets went across his chest. They are built and painted as the other classes' are (above);
from the class's robot definition (`CustomScientist_ROBODEF`) and its sets:

- **His size:** his parts are drawn at 0.7 of their size and his collision is 4.5 m
  tall and 1.75 m round (its `CollisionHeight` 225 and `CollisionRadius` 175), between
  the Infiltrator and the Destroyer, with everything that follows from it.
- **His moves and sounds:** the light moves of `Shared_SCOUT_ROBO_ANIM`, as the
  Infiltrator's (his class's animation sets are the Scout's), and his own sword swing
  (`Scientist_ROBO_ANIM`'s `Melee_Sword_Scientist`, with the light sword's swing); his
  class's sound event set (`CHR_MP_SCIENTIST_1`) gives him the medium robots' steps
  (`BL_FS_MED_BOT`).
- **His camera:** the class's camera set (`Plane_MP_CAMSET`) on foot (in his jet,
  Escalation's: Starscream's and Shockwave's `Plane_Starscream_CAMSET`, the same
  9 m behind and 1.5 m over the jet's middle as the multiplayer set's (the middle
  of its bounds, where the game's pawn is: 1.36 m up for Starscream's), but 75
  degrees wide hovering
  and free within 80 degrees of level, and 100 degrees wide flying); on foot,
  `Plane_OverTheShoulder_STRATEGY`, 6.5 m out (Jazz's 5.5), aimed 2.25 m to the side
  and 1.65 m over the middle of his body, further ahead as the view comes up; the
  Nucleon Charge Rifle's scope takes it in 4.5 m behind him and 2.5 m up. Hovering in
  his jet, `HoverPlane_MP_STRATEGY` follows 9 m behind, aimed 1.5 m over the jet's
  middle, the view held within 45 degrees up or down, at the game's 80-degree field
  of view, and the mouse turns the view as the game's camera turns a vehicle's
  (`TnOrbitRotationCameraBehavior` under the mouse: the mouse's input held within
  1.5, at the strategy's 70 degrees a second across and 35 up and down, doubled by
  its ramp and times the Camera Sensitivity's multiplier): as fast as the mouse
  goes, but no faster than 262.5 degrees a second across and 131 up and down at the
  middle Camera Sensitivity, so a flick can't wrench the jet round or roll it over
  (its body turns after the view, leaning into the turn). Flying, `FlyingPlane_MP_STRATEGY` (the game's flying camera,
  `TnFlyingFollowCameraBehavior`) widens it to 90 degrees over a second and lets the
  view go 80 degrees up or down, and the mouse no longer turns the view itself: it
  sets how fast the view should turn, and the view turns that way as the game's does
  (up to 560 degrees a second yawing and 360 pitching, picking up speed at 720 degrees
  a second each second and damped), rolling into its turns (0.05 of a degree for each
  degree a second it yaws) while the camera swings out to the side of the turn, so
  flying feels like the game's rather than like aiming. Vortex's helicopter has its
  own (its form's camera state, `TnPCS_Chopper`): 9 m and 3.5 m up hovering, 9.5 m and
  3 m flying.
- **His jets:** a jet hovers and flies as the game's do (`TnPlaneForm`, with no
  gravity in either: `TnHoverPlaneSimulation` by the forms' multiplayer blueprint
  `HoverPlane_Physics`, `TnPlaneSimulation` by `Plane_Physics`, Escalation's; Vortex's
  by `HoverHelicopter_Physics` and `Helicopter_Physics`). It is in your hands
  from the moment you start to change into it, as the game's is (its
  `TnTransformation.Execute` activates the jet's form before the change has played,
  and the jet's keys take over at once): it moves, climbs and boosts while the
  robot is still folding up, starting from his own speed, and if Boost is held as
  you change it flies straight away (`TnPlaneForm.OnActivate`). Otherwise it
  **hovers**: W A S D strafe it in any direction in the view's frame
  (up and down with the view's pitch as well) at up to 18 m/s, speeding up at
  27.5 m/s² (Vortex's helicopter 25.5 m/s, at 32), and it holds still in mid-air when
  you let go; **Jump** (Space) climbs and **Descend** (Control) dives, at 13.5 m/s. Its body turns to the view (Vortex's nose 5
  degrees above it: its `PitchOffsetFromCamera`) and leans into its moves (banking as
  it strafes and turns, its nose dipping as it speeds up), as its rigid body does in
  the game. **Boost** (hold the right mouse button) and it **flies**: it thrusts along
  the view up to 40 m/s (at 30 m/s²; Vortex's 42 m/s), its speed across its nose dragged
  away so it carves round as the view turns, banking up to 77 degrees into turns and
  leading them with its nose (the game's `CalculateFinalRotationToMatch`). Letting go
  drops it back to hovering, drifting for a quarter of a second before its grip comes
  back; so does a crash into anything at over 30 m/s (its `ReturnToHoverCrashSpeed`),
  with its crash animation and sound. Floors, walls and ceilings stop it; it neither
  falls nor follows the ground down. With Toggle Vehicle Boost a press starts flying
  and the next one (or a crash) stops it. How high a jet floats is left to the game's
  physics, so, as the tanks do, each rests with its lowest point 0.6 m off the floor
  in its hover pose.
- **Their animations** are the game's: the hover poses blending by how it speeds up
  (forward, back, to the sides and up), the hover bob, the change from hovering to
  flying and back (`Nav_HoverToBoost_VEH`, `Nav_BoostToHover_VEH`), banking left and
  right as it flies (`Nav_Boost_L_VEH`, `Nav_Boost_R_VEH`, by its roll) and its crash
  (`Nav_Collision_VEH`). Their jets are the game's particle systems on their own
  sockets: the hover jets underneath while it hovers (Jetfire's own red ones,
  `hover_flyer01_A_FX` and `hover_flyer02_A_FX`, Kickback's `hover_plane_sm_FX` and
  the others' `hover_plane_FX`; Vortex and Swoop have none), and as it flies its
  afterburners (`Afterburner_A_FX`, `Afterburner_D_FX` and the others' own:
  Silverbolt's, Kickback's, Shockwave's, Vortex's and Sharpshot's), the vapour trails
  off its wings (`Trails_Jet_A_FX`, `Trails_Jet_D_FX`) and its boost trail
  (`Trail_SatHomingRocket_FX`), in the team's energon colour where an effect takes one
  (`TnVehicleFxPlayer.Play` gives every vehicle effect its `Color`). Each
  `hover_plane_FX` is as big as that jet pushes, as the game sizes it
  (`TnPlaneForm.Hovering.UpdateFx` sets its `Size` to `CalculateBoosterFxAmount` times
  its socket's scale: its share of how the jet speeds up and turns, with no gravity,
  since a jet has none), so they flare as it moves off, turns or stops and die away
  while it hangs still; `tools\ue3\particles.py` reads the
  size module that takes it (`SizeMultiplyLife` by a
  `DistributionVectorParticleParameter`) and `levelfx.rs` runs it with the parameter
  the game sets. The glow at the heart of each hover jet is a single mesh that stays
  for as long as the jet hovers: its emitter spawns 100 a second, but its particles
  never die and the game's data has one out at a time (above, *Particles*), as for
  the afterburners' glows. (They used to pile up, up to 2000 copies of the mesh in
  one place for every hover jet, built afresh every frame: what dragged the frame
  rate down while a jet hovered.) On the Customize screen a jet comes out of its
  transformation on the ground and rises to its hover on its jets, as a tank does,
  and hovers there as in play: its hover pose with its hover animation playing on top
  (`ADD_Nav_Hover_VEH`: each jet rocking and rising and falling, Sharpshot's and
  Kickback's wings buzzing and their legs stirring, Swoop's wings beating, Vortex's
  rotor turning), its hover jets burning at their full size (nothing sets their
  `Size` in the garage, so their own size holds, as the game's garage shows them). A
  tank bobs on its jets there as it does in play.
- **His rockets:** in his jet, the left mouse button fires the class's vehicle weapon,
  the game's homing rockets (`TnWeaponHomingRocketVehicle`,
  `HomingRocket_Vehicle_SV_WEPDATA`): each pull of the trigger fires a burst of two,
  0.15 s apart, from the jet's two weapon sockets by turns, and the next burst waits
  a second (its `BurstDelay`) from 0.15 s after the last rocket, as the game's weapon
  states run. It holds 4 (16 more in reserve) and reloads by itself once they are
  gone, in 3 s. Each rocket (`HomingRocketVh_SP_PROJDATA`, its Escalation data: the
  projectile's `Data`, as `TnProjectile.GetProjectileData` picks it, there being no
  `SurvivalData`) flies at 100 m/s for up to 4 s and goes off on the first thing in its
  way: 75 damage to every enemy within 5 m, in full (its `RadiusDamageScale` has no
  scales), and a quarter of it to the Scientist himself if he is that close. Keep the crosshair
  on an enemy (within 7.5 degrees of it, 168.5 m away at most and in sight) for half
  a second and it **locks on** (its `LockOnTime`): the game's lock-on marker closes in
  on the enemy as the lock comes and settles onto it (`mc_markerLockOn`, as the Thermo
  Rocket Cannon's: *The Destroyer's weapons*), the crosshair turns red and the game's
  lock tone sounds, and the lock holds for 1.5 s after the enemy leaves the
  crosshair (`HoldLockOnTime`). As in the game, the lock is only looked for while the
  rockets are ready or firing (not while they reload or wait between bursts) and
  with rockets in the clip. Rockets fired while it is locked on home in on that
  enemy (steering at 800 m/s² and, within 25 m of it, closing in as hard and going
  off a second later if they haven't struck it: `TnProjectileHoming`'s Homing and
  Closing states). Each rocket jolts the camera (its `CameraShakes`) and narrows the view a
  degree for a moment (its `CameraRecoils`). Its sounds (`BL_WPN_GUN_ROCKET`: the shot,
  the reload, the rockets' flight loop and their blast; the ballistic dry fire when it
  is empty; the lock tone `BL_HUD_INTERFACE.HUD_TARGET_LOCK`) and effects (the muzzle
  flash `MuzzleFlash_HomRocket_FX`, the rocket `Tracer_HomRocket_FX` (at 1.25 times its
  size: the rocket's projectile mesh's `DrawScale`) and its blast
  `HomRocket_Explosion_FX`, its fire in the team's energon colour, which the rocket's
  `Parameters` give its effects: `HmProjectile.OnParticleSystemCreated`) are the
  game's, and the HUD shows the game's rocket crosshair (`mc_crosshairRocket`). Online,
  the rockets fly, home and go off on the other players' PCs too.
- **Their sounds:** each jet's form's sound event set: `VEH_JET_01` for most,
  `VEH_JET_GUNSHIP_01` for Vortex's helicopter and `VEH_INSECTICON` for Sharpshot's
  and Kickback's Insecticons. Hovering, the hover flight's engine loops play
  (`HOVER_FLIGHT_ONLOAD`), with its turbine whine as it moves ahead and its hover
  boosters as loud as its jets push (`HOVER_BOOSTERS`, as `Hovering.UpdateFx` works it
  out), a burst as it moves off and as it stops, and its climb and dive sounds as you
  ascend and descend (`HOVER_FLIGHT_ASCEND`, `_DESCEND` and their stops); flying, its
  boost loop and whine (`BOOST_LOOP`, `BOOST_LOOP_WHINE`), settling and rising in pitch
  over the seconds it has flown as their cues' curves have it, with the boost's start
  and end; then its crashes, death and each transformation's own sounds
  (`BL_TRANSFORM.JETFIRE_BOT2VEH`...). The helicopter's rotors and engine and the
  Insecticons' buzzing wings play the same way, each as loud against Jazz's car as in
  the game. Vortex's transformation into his helicopter starts its main engine
  (`VORTEX_BOT2VEH` plays 5.3 s of `JET_HELI_01_ENGINE_MAIN_LP` from 0.8 s in, as the
  helicopter comes together); that recording stops (in a tenth of a second) as soon as
  the helicopter stops hovering: as it boosts off, turns back into a robot or is
  destroyed, for the other players' copies too, and in Customize as it turns back.
  No other vehicle's transformation plays its engine.

What's left out: the Scientist's abilities (he has none; his four weapons are his
own), the jets' dash (an ability in the game, `TnAbilityPlaneDash`), the rockets'
icon in the HUD's ammo notes, and the class's own health and armor (he has Jazz's).
The other players see your jet fly level, without its banking and pitch, and its
hover jets don't flare there (how it speeds up isn't sent). The mass the flight's drag acts on isn't in the game's data (it is its physics
asset's), so a car's 1500 kg is taken. His files add about 190 MB to `assets\models`
(most of it the texture sets).

### Extras and the credits

EXTRAS opens the Extras movie (`ExtrasMenu_GFX`) as the game loads it over the
front end: the front end's teal overlay goes over the background movie, which
keeps playing, and the Extras movie's own background (the same gradient,
multiplied in once more) darkens it further over a second. Its root menu
(`mc_rootMenu`) sits low on the left: the EXTRAS title ($HeaderFont, Register
Light) slides in from the left over half a second with the focused button's hint
in orange under it, and the buttons (the shared `mc_panelButton`, only their
labels shown) sit 27 pixels apart. The selection bar (`ExtrasMenu_GFX_I2F`, a
9-slice) grows under the focused button from a quarter of its width as it moves
there, and the button's label slides left and turns from bright to dark teal. The
game's root menu has Movies, Audio Logs and Credits; the demo has Credits, which
does what the game's does (`StartExtrasMovie("TFOC_Ending_Credits")`): the credits
movie plays over everything, fitted to the window, with its music, while the
menu music fades out and the background movie waits. At its end, or when skipped,
the Extras menu is back and the menu music fades in again. The buttons play the
game's `BUTTON_UP` and `BUTTON_ACCEPT` sounds (`build_sounds.py` includes them).

The credits Bink has six mono tracks; only the first two (left and right) carry
sound, and `build_menu.py` joins them into a stereo Ogg. The picture is encoded
like the background movie but tighter (x264's slowest preset at CRF 26), which
keeps the credits' text as sharp as the Bink's at about 28 MB.

### The pause menu

The game's pause menu is the Scaleform movie `UI_GFxPause_p.PauseMenu_GFX` (in
`mp_shared.xxx`, among others); the demo shows its multiplayer shell
(`mc_multiplayerShell`) the way `src\menu.rs` shows the front end: the menu (rows
24 pixels apart, the same selection bar, drift and title typing as the settings
menus), the TELETRAAN 1 logo sliding into place as the Teletraan emblem slides away,
the warped hexagons and emblem glow behind it and the six pips closing in around it,
with the timings of the movie's timelines. The warped hexagons
(`SharedAssets_GFX`'s `mc_warpedHexagonBackgroundArt`) are a disc 600 pixels across
made of four quarters, each its 256-pixel image (a quarter of the disc, brightest
at one corner) at 117%, mirrored across, up and down or both so that the bright
corners meet behind the logo: a fine honeycomb fading out all round it, under the
emblem's glow. (Before, the demo stretched the one quarter over the whole disc,
which left the glow on its own over coarse hexagons off to its lower right, so the
graphic looked cut off below the logo.) The menu's teal panel (`mc_menuBackgroundArt`, also behind the
settings menus) has a scale-9 grid, as its selection bar does: its image
(`MenuBackground`, an arrow-shaped notch in the middle of its left edge) keeps its
left 22 pixels at their own width and its top and bottom halves at their own height,
and only the one row through the notch's tip stretches, so the panel's left edge
runs straight down beside the menu, the notch's slopes pushed out to 233 pixels above
and below its middle, as in the game (stretched whole, the notch made an angled dent
beside the rows). Its menu is the game's multiplayer one
(`MultiplayerMenuProvider`) without Switch Teams (there are no teams) and Invite
Friends; its Choose Character opens the Choose Character screen (below). `build_menu.py` copies its
images (the logo and emblem from `mp_shared.xxx`'s export, see the script) and
`build_sounds.py` its sounds.

Quit Game asks first, as the game's `QuitToMainMenu` does with its `TnQuitMessageBox`:
the shared message box (`MessagePrompt_GFX`, the same box as the lobby's messages,
with its alert icon) titled Quit Game (`MessagePrompts.LeaveLobby`), with Yes and No
(`ButtonHints.Yes`, `.No`). Its question is the one you asked for: "Your XP progress
will be lost. Do you want to proceed?" (the game's multiplayer asks "All gained XP
will be lost.", `LeaveLobbyXpConfirm`, and its Escalation "Your leaderboard progress
will be lost.", `LeaveSurvivalConfirm`). The host's pause menu has Return to Lobby
above Quit Game, which the game hasn't (its own end-of-match travel back to the
lobby is `TnGame.ReturnToLobby`): it asks in the same box ("The match will end and
all players will return to the lobby. Do you want to proceed?"), then ends the
match (`src\net.rs`'s `end_match`: the roster says the match is over). Every PC,
the host's too, fades to black and comes back in the game's lobby, still connected,
the other players with a message saying the host ended the match, and the host can
start the next one from there. However the match is left (Quit Game, Return to
Lobby, or its end), its sounds go with it once the screen is black, as the game's
change of level stops every sound of the level: the stingers (a wave's starts with
the countdown's "One..." and plays on for half a minute), the announcer, the
weapons and the engines fade out within a tenth of a second, and any it had yet to
start never do, so only the menus' own music and sounds play there
(`stop_match_sounds` in `src\sound.rs`).

While it is up the world is drawn by `src\hologram.rs` and
`assets\shaders\hologram.wgsl`, a post-process pass over the game camera that
recreates the pause chain `UI_GFxPause_p.PauseMenuPostProcess`: its material
`UI_Materials_p.Hologram_Post_MAT` (rebuilt from the material's expression graph
and custom HLSL nodes, with `PauseMenu_Post_MAT_INST`'s parameters: edges found
from jumps in the depth buffer, blue on dark blue with scan lines, and the ring
that sweeps it in from Jazz and out again over a second, as
`TnGameViewportClient`'s Start/Update/StopPauseEffect drive it) and the chain's
depth of field. The game camera's depth buffer is made readable for it.

### The Choose Character screen

In a match the game picks your character with the Scaleform movie
`UI_GFxCustomize_p.CustomTransformers_GFX` (in `mp_shared.xxx`), played with
`selectOnly`: the multiplayer UI controller shows it as the match starts
(`TnUIControllerMultiplayer`'s `InGameLobbyUI`, with no going back) and the pause
menu's Choose Character loads it over the pause menu (`openCharacterSelect`, with
`backout`: Back returns to the pause menu, and choosing closes the pause menu with it,
`onCustomizeClose = closePauseMenu`). `src\choose.rs` draws it through `flash.rs`
in the movie's own numbers: the black that fades in over the world and the
backdrop (the root's shape 424: the band behind the title and the panel behind the
loadout, with its radial gradient and hairlines), the CHOOSE CHARACTER header
sliding in, `mc_customSelection` at (90, 155) with the class labels and their level
bars over the classes' vehicles (each as wide as the class's experience past its
level's start over what the next level needs, `<class>Prog_mc._width`; empty at the
top level, where the game's `kLevelTooHigh` leaves it), the builds' labels (the shared `mc_panelButton`: 23 pixels apart, sliding to the left
edge and darkening over 0.2 s as the selection bar slides under them), and
`mc_characterOverview` at (525, 275): the class's vehicle icon in orange, the
build's name and its class's level, both weapon cards with their wireframe pictures fading in
(the second a tenth of a second after the first, the ability's after 0.35 s) as the
corners round them spread out, and every line typing its new text over the old
(`updateTextField`), with the text fields' glow and shadow filters (drawn by
`flash.wgsl`). The builds come from `builds.txt`, as the Customize screen saved them
(with anything their class's level hasn't opened given up, as there). A slot the
class's level hasn't opened reads "Unlocks at level 5" (or 10), dimmed, and can't be
chosen: the game makes no character for it until then (`GetSlotsForSpecialty`).

**One of each class.** In an online match each class can be played by one player
only, as each of Escalation's characters can be claimed by one player only
(`TnPlayerControllerSurvival.ServerTryClaimCharacter`): a class another player has
chosen is dimmed on everyone else's screen with its label and builds, as the panel
buttons are when disabled, and can't be chosen (if it is taken while the screen is
up, the focus moves off it). Each PC sends its player's class with his Jazz
(`src\net.rs`, protocol version 25), and a class is free again once its player
chooses another or leaves. Should two players choose the same class at the same
moment, the one in the lower slot of the lobby keeps it, and the other is told who
took it (the game's notice at the bottom right: the class over "Optimus chose the
Destroyer first.", with that player's name): as the match starts his screen comes
back for him to choose again, and the countdown waits for him; in the match he
keeps the character he had before.

Choosing a build (`SelectCharacter`) makes it Jazz's character: `src\main.rs` gives him
its two weapons as he spawns. As the match starts, the starting scoreboard's
countdown starts with the choice and he spawns with it as the countdown runs out
(below); until then the screen shows the map through the level camera, as the game
does, with no Jazz on it. Chosen from the pause menu, a different character is used when Jazz
next respawns, as the game's notification says (`TnUIControllerMultiplayer.
OnCharacterSelected` shows `UI_GFxOnlineNotifies_p.OnlineNotify_GFX`'s message at
the bottom right: the character's name over "Selected character used on respawn.",
its panel opening, its corner strokes pulsing cyan, fading out after four seconds). Every
class's builds can be chosen, each with its own two weapons (the demo has all of
the game's) and its own class ability (the demo plays all eight; the EMP
Grenade, the Hover Slam, the Flak Shield and the Battle Sentry open at their class's
level 11). Like the game's on PC, it
shows no hint buttons: its hints (Accept, and Back from the pause menu) have no
clickable buttons on PC.

`build_customize.py` makes its images in `assets\choose`: the textures straight from
`mp_shared.xxx` and `TransGame.xxx` in `CookedPC` (no umodel export needed), and the
vehicle icons, the class list's vehicles and the backdrop drawn from the movie's
vector shapes. Its text goes into `customize.txt`. Without `assets\choose`, the
countdown starts as the match does, and Jazz begins it with the Neutron Assault
Rifle and the Scatter Blaster, as before.

### The starting scoreboard

Before a player spawns the game shows him the map through its level camera: the
`CameraActor` the level's `TnWorldInfo` names as its `LevelCamera`
(`TnSpectateTargetChooser.GetLevelCamera`), one in each map's BASE package (on
Downfall it looks up past the containers at the docking ring, as in the game's
videos). `build_map.py` writes it into the map's points file (`camera x y z heading
pitch fov`: its place, the way it looks, and its field of view across, the
`CameraActor`'s 90 degrees), and `src\main.rs` puts the view there, with the field of
view across the window as the game's, while the Choose Character screen and the
countdown are up; the player's Jazz isn't on the map meanwhile, nor are the enemies
and weapons, which come as the match begins (`start_match` runs again then:
the match's clock starts with it).

Once he has chosen, the multiplayer UI controller's `GameStartUI`
(`WaitingOnGameStart.OpenGameStartUI`) shows `UI_GFxPreGameCountdown_p.PreGameCountdown_GFX`
(in `mp_shared.xxx`), which loads `UI_GFxPlayerList_p.PlayerList_GFX` in its `InGame`
view. `src\pregame.rs` draws both through `flash.rs` in the movies' numbers (stage
pixels, frames at 30 a second):

* the view darkened through sprite 4's gradient (shape 3: #2899b5 at the top and
  bottom, #014a5c in the middle) in Flash's **multiply** blend mode, brought in to 60%
  over a second (`flash.rs` now has the movies' add and multiply blend modes);
* the bands of light, all #50b5d5 at 25% in the **add** blend mode: `hints_mc`'s
  notched band and the box under it at (-103, 590), and sprite 31's band under the
  countdown and the long band over it at (-103, 172), each widening frame by frame as
  its timeline has it (scale-9: their notched and slanted ends keep their shape), and
  Escalation's mode icon (`mc_gameModeIconSV`) at 280% by them, turning in from -60
  degrees as it fades in over half a second;
* the countdown (sprite 23): the localized "MATCH STARTS IN" (#50b5d5) right-aligned
  to the seconds (white, 14 pixel Register Demi Bold), sliding along as the number
  changes (0.2 s); below 10, each new number pops from twice its size with a white
  glow (0.5 s). Until it starts, "WAITING FOR PLAYERS..." shows in its place (the
  movie's `waiting_mc`, while `IsCountingDown` is false); as it starts, it rises 10
  pixels into place and fades in while the waiting label fades out (0.5 s). The
  game runs its countdown on the server (`TnGameReplicationInfo.CurrentCountdown`);
  here the host runs it: it is 15 seconds, starting once every player in the match
  has chosen his character (playing without the network, as he chooses), and from
  10 down each second plays the game's `LowCountdownTickSound`
  (`MP_COUNTDOWN_TICK`: `BL_LVL_MP.MP_COUNTDOWN_BEEP`);
* the players' list at (320, 162), from its first update a second after the movie
  opens: the team header (`mc_teamHeaderCell`'s `blue` frame, the player's own team
  in the default colouring: #a7e6fe with a #0980d7 glow) with the team's score at
  130%, the column header (LEVEL, NAME, SCORE, KILLS, DEATHS, REVIVES, MONEY: 12
  pixel #999999), and a row for each player 20 pixels apart, laid out as
  `CreateRowElements` lays them (each cell after the one before, the icons by their
  spacing alone), with the connection bars. The game's own starting scoreboard loads
  the list in a single-team game's in-game view, the ability first (ABILITY); the
  demo shows the scoreboard in a match's **LEVEL** there instead, as everywhere else
  (the symbol of the class each player has chosen and his level in it: see "The
  scoreboard", below). The list has the input, so the player's own row is focused:
  the selector highlight (`PlayerList_GFX_I1C`, as wide as the list) fades in under
  it and its text goes #0a2828. The movie's game mode title (`gameMode_txt`) stays
  empty, as the game leaves Escalation's `GameModeFriendlyName`.

At zero the movie closes and the match begins for that player (`HandleStartGame`):
Jazz spawns at his start with his character, the HUD comes up and the announcer and
the music start. Online, each PC says when its player has chosen (the snapshot's
`CHOSEN` flag); once the host has heard it from everyone in the match (a player
who leaves before choosing isn't waited for), it starts the countdown and sends it
with the lobby every half second, and each PC counts down with it between those, as
the lobby's own countdown is kept in step: the numbers tick over together and the
match begins on every PC at once (protocol version 14). Until then each Jazz is
sent as waiting (the `WAITING` flag): no PC shows him. Esc still opens the pause menu over the countdown (which runs on, as the game's
does), and Choose Character from it doesn't hold the countdown up.

`build_pregame.py` makes its images in `assets\pregame\img` straight from
`mp_shared.xxx` (the bands drawn from the movie's vector shapes, the gradient from
its stops, the highlight from its texture, and the classes' symbols for the
scoreboard below from the shared icons' shapes); the mode icon and the connection
bars are the lobby's. Without them the countdown and the list still show, without
the art:

```
cd Documents\foc_esc\tools
python build_pregame.py "C:\...\TransGame\CookedPC" ..\assets
```

### The scoreboard

In a match, **Tab** (the layouts' Scoreboard key) brings up the game's scoreboard and
Tab again puts it away, as `TnHUD.ShowScores` toggles it: its `SetShowScores` starts
the movie `UI_GFxInGameStats_p.InGameStats_GFX`, which loads the players' list
(`PlayerList_GFX`) into it, and hands it the players. The game plays on under it.
`src\scoreboard.rs` draws it through `flash.rs` in the movie's numbers:

* the view darkened through the starting scoreboard's gradient in the multiply blend
  mode, brought in to 60% over a second (the movie's sprite 2, stretched over the
  window);
* at the top, centred (sprite 6 at x 560), the mode on the map in orange (#ff9332):
  the movie's "`g on `m" (`Shared.GameModeOnMap`) with the mode's name in white and
  the map's, in upper case; Escalation's name is left empty (`GameModeFriendlyName`),
  so it reads "ON DOWNFALL". Under it the mode's rules in blue (#50b5d5): "Survive
  waves of attacks" (`TnOnlineGameSettingsSV.RulesFormat`);
* the players' list at (290, 218), at 85%, drawn as on the starting scoreboard: the
  team and its score, the columns, a row for each player with his connection's
  bars, your own row highlighted, the rows ranked as the movie ranks them (score and
  kills high to low, then deaths low to high, then the names). The movie loads the
  list in its in-game view, which starts with the Infiltrator's ability; here its
  first column is the lobby's **LEVEL** instead (`PlayerLevel`, 40 pixels wider, the
  rest moving along with it): the symbol of the class the player is playing (the
  shared icons' `mc_specialtyIcon<class>`, at 55%) and his level in that class, in
  the row's colour (#5ad1fe, or #0a2828 on your highlighted row), the symbol growing
  in and the number sliding in from the left at twice its size over 0.4 s as the
  scoreboard opens or the player changes class, as the lobby's
  `mc_playerSpecialtyLevelAnchor` does.

The numbers are each player's match so far, as the game keeps them on his
`TnPlayerReplicationInfoSurvival`: SCORE is all the energon he has earned (Escalation's
`AddScore` adds what he earns to his score as to his money, and spending takes it from
his money only), KILLS the enemies he finished, DEATHS how often he died, REVIVES the
teammates he revived (`AddRevive`) and MONEY the energon he has left to
spend. Each PC counts its own player's and sends them with his Jazz, along with the
level of the class he plays (protocol version 25), so every PC's scoreboard shows
everyone's. The team's score stays 0, as in the game: Escalation's scripts never add
to it (`TnTeamInfo`'s score is only reset; its kill rule, `TnGameRules_ScoreKillsSV`,
gives the team nothing).

Esc puts it away too, with the movie's own back sound (`closeInGameStats`:
`BUTTON_REENTER_GAMEPLAY`), rather than opening the pause menu; the pause menu and the
Choose Character screen put it away, and it doesn't come up before the match has
begun. Its button hints (`staticButtonHints_mc`) are left out, as on the starting
scoreboard.

### The Controls settings

SETTINGS > Controls is the settings movie's `ControlsMenuProvider` as the PC
version builds it (with its Mouse Sensitivity slider, and the LS / RS Button Delay
under its Xbox 360 name), laid out like the other settings menus. Each setting is
kept in `settings.txt` and does what it does in the game:

- **Active Game Controls**: Keyboard/Mouse, or Controller, which hands Jazz to a
  game controller (see *Playing with a controller*, below) while one is connected;
  as in the game, the keyboard and mouse don't play him then (the menus take
  either), and with no controller connected they do.
- **Controller Layout** (with Active Game Controls on Controller) opens the game's
  screen: the Robot, Vehicle and Thumbsticks layouts, and the controller
  with what each control does (see *Playing with a controller*, below).
- **LS / RS Button Delay**: on, a stick's click (Change Form, Melee) counts once it
  has been held a fifth of a second (the game's `ThumbStickHoldDelay`), so a hard
  push on a stick doesn't change form or punch. **Camera Sensitivity** sets how
  fast the right stick turns the view (from half to twice the game's look speeds;
  the middle setting is 1.25 times). **Controller Vibration** is kept (the demo's
  controller doesn't rumble).
- **Toggle Vehicle Boost**: Hold boosts while Speed Boost is held; Toggle starts
  boosting with one press and stops with the next (or on a crash, or out of the
  car).
- **Robot Camera**, **Vehicle Camera** and **Jet Camera**: Inverted turns the mouse's
  (and the right stick's) up and down round in that form (Jet Camera in a Scientist's jet: the game picks the
  setting by the vehicle's type, `TnPlayerController.UpdateInvertMouseByVehicleForm`).
- **Mouse Sensitivity** runs from 3 to 120 in steps of 3 like the game's slider;
  60, the game's own default (`MouseSensitivity` in its input settings), is the
  demo's usual mouse speed.

**Keyboard/Mouse Layout** is its sub-menu (`KeyboardMouseLayoutMenuProvider`): the
game's keyboard layouts under **Select Layout** (Default, and the keyboard schemes
`TransInput.ini` adds, named in `TransGame.int`: Annihilator, Quickdraw,
Wheelless, Rebel and Southpaw), **View Layout** (Robot or Vehicle: the game's
Grimlock view, for the campaign, is left out), and
under them the table the game shows (`mc_keyboardMouse`: each action on the left,
its key on the right, 20 pixels apart, from the movie's `DefaultKeyboardLayouts`
and `getKeyboardLayout` tables). The game can only show a layout; the demo plays
with it (`src\controls.rs`: the robot's keys on foot, the vehicle's in the car),
and lets you change any key in it. Choose a row (Down past View Layout, or the
mouse) and press Enter or click it: the key says "Press a key" until you press
the new one, a mouse button or the wheel (Escape, a gamepad's B or a click on Back
stops waiting). Move takes four keys, forward, left, back and right (the arrow
keys always move too). If another action had that key, it takes the old one in
exchange. A change makes a **Custom** layout, which Select Layout then offers with
the game's; picking one of the game's layouts again keeps Custom for later, but
changing a key while one of the game's layouts is showing starts Custom afresh
from that layout. Turn/Look (the mouse) and Pause (Escape) can't be changed;
Escape and the arrow keys can't be given to anything else, and the wheel can't be
given to what is held down (firing, fine aim, sprint, boost, picking up). The
robot's table has one row the game's doesn't, Throw
Grenade: the game binds G to it in every layout without listing it. In two places
the game's own tables don't match its bindings, and the demo goes by the
bindings: with Annihilator, C changes form in the vehicle too (its table says F),
and with Wheelless, 5 shows the scoreboard in the vehicle too (its table says
Tab). Dash(Press)/Sprint(Hold) is one key: a tap dashes as you let go of it, and
held for a fifth of a second Jazz sprints instead, without dashing (the demo's
own timing). The HUD's "Hold E to pickup" (and "Hold E to purchase") names the key
Interact/Pick Up is on, and CLICK TO PLAY lists the keys of the layout in use.

The layout and any keys of your own are saved in `settings.txt` with the other
settings (`keyboard_layout`, and `bind_<form>_<action>` lines in Bevy's key names,
such as `bind_robot_fire MouseLeft` or `bind_robot_move KeyW,KeyA,KeyS,KeyD`).

The settings menus' positions were measured against the game's Controls menu:
each sub-menu drifts from where the menus were when it opened (`mc_menu`'s
`initPosY`), the texts sit at their fields' placement points, the steppers'
values keep their colours on the focused row with the movie's dark glow around
them, and the sliders' track and bar keep their ends' size as they stretch (the
movie's scale-9 grids) with the bar 10 pixels plus 190 times the value's share
long, as the game's `Slider` draws it. The selection bar keeps its ends' size the
same way (`mc_selectionBarArt`'s scale-9 grid in `SharedAssets_GFX`: 10 pixels on
the left, 2 on the right), in the settings menus and the pause menu, so its
notched left end stays clear of the focused row's text as in the game.

`build_menu.py` reads the Controls text from UIText.int (the settings' names and
values, the tables' actions and key names) and the layouts' names from
TransGame.int, into `menu.txt`.

### Playing with a controller

With **Active Game Controls** on Controller (SETTINGS > Controls), a game
controller plays Jazz: an Xbox controller, or any other the system's gamepad
support knows (Bevy's, through gilrs: XInput and DirectInput controllers on
Windows; the first one connected plays). It plays him as the game's PC version
does, by the layouts chosen on the Controller Layout screen (`PadBindings` in its
input settings and its control schemes, `src\pad.rs` and `src\padlayout.rs`).
By the default layouts:

| Control | On foot | In a vehicle |
|---|---|---|
| Left stick | Move | Move (hovering) |
| Right stick | Look around | Look around; on its wheels, steer (and look up and down); a flying jet: its yaw and pitch |
| Left stick click | Change Form | Change Form |
| Right stick click | Melee | |
| A | Jump | Jump; a jet: climb |
| B | Switch Hands (down: hold to self destruct) | A jet: dive |
| X | Tap: Reload. Hold: Interact/Pick Up (pick up, buy, revive, open) | Reload |
| Y | Swap Weapons | |
| LB | Dash (tap) or Sprint (hold) | |
| RB | Ability | Ability |
| LT | Fine Aim | Speed Boost (Toggle Vehicle Boost: held or toggled) |
| RT | Fire | Fire |
| D-pad left, right | Assault T.E.C.H., Utility T.E.C.H. | |
| Back | Scoreboard | Scoreboard |
| Start | Pause | Pause |

Spectating after death, RB and LB go through the players to watch. The menus, the
pause menu, Choose Character and Customize take the controller as before (the
D-pad or left stick, A, B, Start).

- The sticks have XInput's dead zones (about a quarter of the way over: 7849 and
  8689 of 32767), and a trigger counts past 30 of 255 (XInput's threshold). Past
  its dead zone the left stick moves Jazz at his speed whichever way it points, as
  the keys do.
- The right stick turns the view as the game's cameras turn it for a stick
  (`TnOrbitRotationCameraBehavior`, with each camera strategy's look speeds): all
  the way over, the robot's view turns 50 degrees a second across and 25 up and
  down (times 1.25 at the middle Camera Sensitivity), building up over 0.4 s to
  3.5 times that while the stick is held well over (its ramp, from 70% of the way
  over; a sudden flick of the stick starts it over); 25 and 15 fine aiming (the
  Photon Burst Rifle's scope 14 and 8, the Nucleon Charge Rifle's 6.5 and 3.25),
  20 and 5 sprinting (each class's own sprint camera's: the Destroyer's and
  Titan's 16 and 5, the Scientist's 18 and 5; ramped to 2.5 times); a hovering
  car's or truck's 50 and 25, a tank's 45 and 15, their ramps over 0.3 s; a
  hovering jet's 70 and 35, ramped to twice that over 0.4 s (`HoverPlane_MP_STRATEGY`).
  A change of speeds (fine aim coming up) blends in over half a second.
- On its wheels a car or truck steers by the right stick across, as the game takes
  it (`PlayerInCarForm.SetLocalInputs`: the stick as the steering), and a flying
  jet takes the right stick as its yaw and pitch (`PlayerInPlaneForm`), the flying
  camera following as it does for the mouse.
- X is the game's `XButtonInputHandler`: let go within 0.3 s it reloads; held, it
  is Interact/Pick Up at once (so a door or an Armory Recreator answers as it goes
  down, and a pick-up, purchase or revive goes on while it is held).
- While a controller plays, the HUD shows its buttons as the game's PC version does
  with one (`PreferGamepad`), in the game's own button pictures: RB on the ability,
  the equipment slots' D-pad arrows, X on the RELOAD prompt, LT on the grenade
  launcher's DETONATE and RT on the Combustor's CLUSTERBOMB. The contextual prompts
  name the button in their text: "Hold X to pickup" and "Hold X to purchase" (the
  equipment's D-pad directions in the purchase lines), HOLD B TO SELF DESTRUCT, and
  LB/RB to cycle the players watched; CLICK TO PLAY doesn't come up.

**The other layouts** (SETTINGS > Controls > Controller Layout; the screen shows
each one's controls on the game's controller picture):

| Layout | Changes |
|---|---|
| Robot: Tactician | B: Melee. Right stick click: Switch Camera |
| Robot: Striker | Y: Change Form. D-pad up: Swap Weapons. Left stick click: nothing |
| Robot: Charger | LB: Change Form. Left stick click: Dash (tap) or Sprint (hold) |
| Robot: Southpaw | LB: Ability. LT: Fire. RB: Dash/Sprint. RT: Fine Aim (Speed Boost in a vehicle) |
| Robot: Hoarder | X: Change Form. D-pad up: tap to Reload, hold to Interact/Pick Up. Left stick click: nothing |
| Vehicle: Seeker | A jet climbs on B and dives on A (a car still jumps on A) |
| Thumbsticks: Southpaw | The right stick moves, the left looks around |

The game's Grimlock layouts (Default, Rebel) are for the campaign's Grimlock, and
the demo leaves them out.

Not yet: vibration, and the game's aim assist for a controller (its cameras'
target stickiness and slowdown).

### The Escalation lobby and online play

**To play together:** one player chooses ESCALATION, then **Host a Game**. A
**Game Password** prompt comes up first: type a password and choose Ok, and only
players who give it can join; leave it blank (and choose Ok) and anyone can. The
ticker under ESCALATION says "WELCOME TO FALL OF CYBERTRON MULTIPLAYER." The others
join at the host PC's address on the network (run `ipconfig` on it: its IPv4
Address, such as `192.168.1.20`): they choose ESCALATION, then **Join Game**, type
that address (the port
can be left out when it's 7777) and choose Join; "Connecting to Host" shows until
the host answers, then the game's lobby, with everyone in its player list. A game
with a password asks for it first ("You need to enter a password to join this
game."); a wrong one is turned away ("The password you entered is incorrect.", in
orange) and asked for again. The host chooses the difficulty and
**Start Game**; after a five-second countdown the match starts on every PC: each
player chooses his character, and once everyone has, the starting scoreboard's
15-second countdown runs on every PC together and the match begins. To play alone,
host a game and start it.

The demo talks over **UDP port 7777**. On the same network that just works
(Windows asks to let the demo through its firewall the first time someone hosts;
allow it on private networks). Over the internet, the host's router must forward
UDP port 7777 to the host's PC, and the others join with the host's public IP
address. To try it on one PC, run the demo twice (two terminals, `cargo run` in
each): host in one and join `127.0.0.1` in the other. Each player's name is
Bumblebee until he sets his own with **Display Name** (in the party lobby
or a game's): up to 16 letters, digits, spaces and symbols, shown in the lobby's
player list and, in a match, over his Jazz on the others' screens (and under
"CURRENTLY SPECTATING" when he is watched). A
new name reaches the others at once, in the lobby. The name and the last address
typed are remembered in `settings.txt` (passwords never are).

Passwords are up to 32 letters, digits, spaces and symbols, and match exactly
(capitals count; spaces at either end are dropped); what is typed shows as
asterisks. The password itself never goes over the network: the host answers a
player's hello with a random challenge, the player's PC signs it with the
password (HMAC-SHA256) and sends back only the signature, which the host checks
against its own. After a wrong password the host ignores that address for a
second, so passwords can't be guessed quickly. The prompts' text is the game's
own, from Unreal's access-control text in `Engine.int` ("Game Password", "If this
password is set, players will have to enter it to join this server.", and the two
messages above), which `build_menu.py` puts into `menu.txt` with the rest. The rest
of the traffic is still unencrypted, as before.

What goes over the network (`src\net.rs`): the host keeps the lobby (who is in
which of the four slots, the difficulty, the countdown) and passes everyone's
packets on to the others. In a match each PC sends its own Jazz 30 times a second
(where he is, which way he faces and looks, his form, stance, moves and their
timings, his health, his car's state) together with what has just happened
(animations started, weapons switched, hands swapped, car jumps, his shots with
what they hit, grenades, melee blows, and the sounds he made), each event repeated
for a quarter of a second so that a lost packet loses nothing. The other PCs play
that out on their copy of him (`net_apply` in `src\main.rs`), smoothing between
packets. The host's AI plays the enemies on every PC, its enemy packets carrying
them 20 times a second (see **The enemies' AI**); the others' shots and grenades
damage the enemies on your PC too (each PC pays its own player his share of each
kill). Each PC still runs its own weapon on the ground, health stations and energon
counter: a weapon another player swaps for is still there on your PC. The difficulty
scales what a kill pays (Easy 0.75, Normal 1, Hard 1.25 times). A player who leaves (or quits the match) drops out of the others'
games; when the host leaves, the others are back at the main menu with the game's
"Connection Error" message.

The lobby is the game's Escalation lobby movie (`UI_GFxLobbies_p.MPLobby_GFX`, in
`UI_Lobby_ESC_m.xxx`), with its player list (`UI_GFxPlayerList_p.PlayerList_GFX`)
and the shared prompts (`TextPrompt_GFX` for Join Game, Display Name and Game Password,
`MessagePrompt_GFX` for errors, in `UI_GFxShared_p.xxx`), laid out by `src\menu.rs` from their timelines
and scripts as the main menu is: the cloud tiles slowly turning into each other
under a shade that darkens the screen towards its bottom, the panels, the title
and subtitle sliding in, the ticker scrolling its message, the map's frame opening
out (wide, then tall) around its picture, the menus of the shared panel buttons and
selectors (rows 25 pixels apart, the focused row's label sliding left and darkening
on the selection bar, Up/Down going round past the rows that can't be used), the
focused row's hint, the player list with its team heading, columns and connection
bars, the game mode's icon turning in behind it, the pulsing status, and the
countdown's number popping each second. **Select Map** steps through the maps
that have been built (`MAPS` in `src\main.rs`): **Downfall** (by the game's data
`ESC_Iacon`, its map 1603), **Oblivion** (`ESC_Ark`, map 1600), **Ancients**
(`ESC_Temple`, map 1602) and **Ignition** (`ESC_Gladiator`, map 1601), with each
one's picture (`mapImg1603`, `mapImg1600`, `mapImg1602`, `mapImg1601`) and name from
the game. The player list is headed by the team the map's Escalation is played by
(`MapInfo::decepticons`): on Ancients and Ignition the Autobots (the team cell's
`autobot` frame: #ffc1c1 with a red glow), on Downfall and Oblivion the Decepticons
(its `decepticon` frame: #dfdfff with a wider purple glow, #6633cc), the other
players' names in the same colour. The host's choice goes to those who join
with the rest of the lobby, and each PC puts the map in play as the countdown
starts, so that it has loaded by the time the match does. The game's map vote and
matchmaking aren't in the demo.

**Prime Mode** is a row of the party lobby here (the game's multiplayer lobby has it
in its Teletraan 1 menu, `mc_menuTeletran1`): Join Game, Host a Game, **Prime Mode**,
Display Name, Settings. Until every class is at level 25 it is locked (white at half,
with the game's lock, `mc_lockPanelButton`) and only says what it needs: "Achieve max
level in all classes to access Prime Mode." Then it asks the game's question
(`PrimeModeMessage`, with Confirm and Cancel), and once Prime Mode is on the row is
gone (`updatePrimeModeActive`). A Prime has the game's Prime medal by his name in the
player list (`mc_awardPrimeMode`: its star turning over its ring), and the others see
it too: each player tells the host, which passes it on with the names (the
network's version is now 36, so every PC needs this build to play together).

`build_menu.py` makes the lobby's images and text along with the menu's: export
`UI_Lobby_ESC_m.xxx` whole and the player list's three connection bars and the Prime medal's two images
from `mp_shared.xxx` (with the pause menu's images), then pass both folders:

```
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu UI_Lobby_ESC_m.xxx
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu mp_shared.xxx PlayerList_GFX_I51 Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu mp_shared.xxx PlayerList_GFX_I53 Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu mp_shared.xxx PlayerList_GFX_I55 Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu mp_shared.xxx PlayerList_GFX_I27 Texture2D
umodel -game=trans -export -png -path="C:\...\TransGame\CookedPC" -out=C:\menu mp_shared.xxx PlayerList_GFX_I2B Texture2D
python build_menu.py "C:\...\Transformers Fall of Cybertron\TransGame" C:\menu\UI_FrontEnd_m\Texture2D ..\assets C:\menu\mp_shared\Texture2D C:\menu\UI_Lobby_ESC_m\Texture2D
```

```
assets\menu\img\lobby_*.png   the clouds, panels, selection bars, map frame, shade and
                             picture, connection bars, spinner, Prime Mode's lock and the
                             Prime medal; the game mode's icon and the selectors' arrows,
                             drawn from the movies' shapes
assets\menu\img\prompt_*.png  the prompts' icons, drawn from the movies' shapes
```

The prompts' and lobby's text comes from UIText.int as the menu's does, with a
few strings from the game's other localization files (`Engine.int`'s connection
errors, `TransGame.int`'s Escalation rules line, lobby status and player-list
columns and the map's name, `TransContent.int`'s team name). `build_sounds.py`
adds the lobby's sounds (`BUTTON_DOWN`, `BUTTON_BACK`, `BUTTON_START`, the
difficulty selector's, the locked item's and `MULTI_MATCH_COUNTDOWN`).

### Up to four players

The game is built for up to four Jazzes at once (`MAX_PLAYERS` in `src\main.rs`),
as the first step towards co-op over the network. Each Jazz is an entity with his
own state (`Player`), his own robot and car models, and a `PlayerInput`: what his
player asks for this frame (the movement keys, the buttons held and pressed, where
he looks and his view, the ray shots and grenades follow). Every system that moves
or fights reads only that input, for each Jazz in turn, so a Jazz can be played
from anywhere: the one on this machine (`LocalPlayer`) gets his from the keyboard
and mouse (`read_local_input`); online, the other players' Jazzes are played out
from their own PCs' packets instead (see above). The camera, the HUD and the car's
engine sound follow the local player (each of the two with split screen, below); the parts of each Jazz's models (his
animation players, hands, guns, forearms and tyres) are tagged with the Jazz they
belong to (`JazzOf`) as they load, so each player's animations, guns and car are
his own.

The players start at the map's own player start (its `TnTeamPlayerStart`, the
`start` line of `levels\<map>_points.txt` with the way it faces: on Downfall, in
the start area, facing up the street towards its gates). The four players start
there and on open floor around it,
3 m out (then 5 m), with no wall in the way (`find_player_starts`), all facing
that way with the camera behind them as each match starts, and each comes back to
his own spot when the team's round ends after he died. The energon count is shared. Sounds a Jazz makes play as
the camera hears them for the local player and from where he is for the others.

Online, the other players' Jazzes (`RemotePlayer`) are the players themselves,
each in his own slot's spawn point (see above).

### Split screen

Two players can play on one PC, the window shared between them; the demo's split
screen is built from the game's own pieces (its HUD, its cameras, its menus).

**To play together:** in the party lobby (ESCALATION), while a controller the
second player can join on is connected (one other than the first player's),
"Press START to join" shows under the first player's name in the party's list. The
second player presses **Start** on it and is in the party, listed there as
**Player 2** (B on his controller takes him out again). Then the first player
chooses **Offline Game** or **Host a Game**, and the game's lobby opens with
Player 2 already in it. He stays in the party when they go back to the party
lobby, and leaves it when the party lobby is left for the main menu or his
controller is unplugged there. (Joining a game online with **Join Game**, Player 2
stays behind in the party: split screen is for games this PC hosts.)

He can also join in the game's lobby itself: the first player hosts a game
(ESCALATION, **Host a Game**). In the game's lobby, Start Game's hint (under the
menus while Start Game is selected) adds "Split screen: press START on another controller to play
together.": the second player presses **Start** on a
controller of his own and joins as **Player 2**, listed in the lobby's player list
under the first player, and the hint becomes "Split screen: Player 2 presses B to
leave." (B on his controller takes him out again). The first player keeps the
keyboard and mouse, or the first controller when Settings > Controls' Active Game
Controls is on Controller (then Player 2 needs a second one); Player 2's controller
is his alone, and the menus don't take it. The host starts the game as usual.

As the match starts the first player chooses his character on the Choose Character
screen, then the screen comes up again for Player 2, PLAYER 2 over it, for his
controller (the builds are the first player's: the progress file is his). In the
match each player has his own half of the window, one above the other or side by
side (**Settings > Display > Split Screen**: Top and Bottom, or Side by Side, saved
in `settings.txt` as `split_screen`), a thin black line between them. Each half has
its player's camera, with the game's field of view across it as on a screen of an
ordinary shape (a half above or below sees a little farther across, a half beside
the other a little less, rather than a squeezed or stretched picture), and his
whole HUD: health and shield, ammo and weapons, energon, his prompts, crosshair and
hit marks, the messages and awards, the kill feed and the markers, laid out in his
half. Each player has his own energon to spend at the stations, pods and doors, and
his own voice for his lines (a kill, a reload, a hit). Start on Player 2's
controller opens the pause menu for him (its Choose Character is his); the game
goes on behind it for both, as online. The maps' and the weapons' effects are drawn
for each view: the sprites, trails and beams turned to each camera, the lens flares
in each, an effect on one player's screen (blood, a shock) on his alone; the guns'
impact sparks and the cars' dust turn to the camera nearest them. A sound is heard
as loud as by the camera nearest it, and the live lights and the characters'
shadows from the lights are those nearest either camera. The waves, the team's
lives and the revives count both players: a downed player can be revived by the
other, and a dead one spectates him. Player 2's experience isn't kept: the progress
is the first player's alone. While Player 2 is in, the game is closed to players
online (one who tries is told the game is full), and he can only join a lobby
nobody online has joined. Two views draw the game twice, so a frame takes longer
than with one (see the What's new item on the second view's cost).

How it works (`src\split.rs`, new): each of this PC's players has his seat (0 the
first, 1 Player 2), and so has everything that is his: his Jazz (a `LocalPlayer`
like the first player's), his camera (made from the first player's as Player 2
joins, so it has the map's fog, the Graphics level's effects and the rest) and his
HUD (a second copy of the HUD's movie on a layer of its own, `src\hud.rs`). Both
cameras draw into a picture of their own the window's size (or the Render
Scale's), each into his part, and a camera puts it on the window; each HUD's camera
draws over its player's part after that (`src\graphics.rs`: cameras sharing a
picture each work their effects over all of it, so in the window the first
player's would show over the guest's half). What one player has in the game
(his HUD's state, his energon and
scores, his character, his spectating, his dialog) Player 2 has a copy of
(`split::Second`), and the systems that keep it do so for each seat. Player 2 takes
a slot in the game as a player online would (`src\net.rs`'s guest): the lobby
lists him, the host's waves and lives count him, and the match waits for his choice
of character too. The effects each view sees are on a render layer of its own: the
first player's camera sees the effects as they are, and Player 2's sees a copy of
each made for his camera where they look different from another place (sprites,
trails and beams) and the same elsewhere (`src\levelfx.rs`'s `Twin`). Sounds are
heard by the camera nearest them (`src\sound.rs`'s `nearest_ear`).

### Changing the arena

The layout is in `setup_level` in `src\main.rs`: lists of positions for the
walls, vents, pipes and piston towers. umodel exports individual meshes, not
whole maps, so this is where you place them. Piston towers are also added to
the collision list automatically.

### If something looks wrong

The settings at the top of `src\main.rs` are there for this:

- Model **100x too big**: set `CHARACTER_SCALE` to `0.01`.
- Model **faces sideways or backwards**: set `MODEL_YAW_OFFSET` to
  `std::f32::consts::FRAC_PI_2`, `std::f32::consts::PI` or `-std::f32::consts::FRAC_PI_2`.
- Map **grey / untextured**: the maps' textures are KTX2 files in
  `assets\models\downfall\`, `assets\models\oblivion\`, `assets\models\ancients\` and
  `assets\models\ignition\`,
  which Bevy reads with its `ktx2` feature (on by default) on any GPU with BC
  texture compression (all PC graphics cards); check that the folder is next to its
  `.glb`.
- **Select Map offers fewer maps**: the others' files aren't there (Downfall's
  `assets\models\downfall.glb` and `assets\levels\downfall_col.bin`, Oblivion's
  `oblivion.glb` and `oblivion_col.bin`, Ancients' `ancients.glb` and
  `ancients_col.bin`, Ignition's `ignition.glb` and `ignition_col.bin`); the terminal
  logs the map put in play.
- **The terminal says a collision file is "of an unknown format"**: it is from an
  older build of the demo (Downfall's used to be Ignition's); rebuild the map with
  `tools\build_map.py`, or copy the new `levels` files over.
- **No sound**: check that `assets\sounds\sounds.txt` exists; the terminal
  logs `Loaded 45 sound cues` at start-up when it does.
- Model **missing or invisible**: check the terminal; the demo logs
  `models/xxx.glb not found` when it can't see a file.

## Keep the game's files private

The extracted models and textures belong to the game's publisher. Using them
from your own copy for a personal project is common practice in the modding
scene, but don't upload or share the `assets\models` folder or a build that
contains it (the same goes for `assets\sounds`, `assets\hud`, `assets\menu`,
`assets\customize`, `assets\choose`, `assets\levels` and `assets\textures`, and for
`extract_work`, where `tools\extract_game.py` keeps its exports while it runs). The
code in `src` and `tools` is yours to share: anyone with the game can build the
assets from their own copy with it (see "Build the assets from your game").

## Ideas for next steps

- **The enemies' AI:** the waves bring the enemies in as the game does, but they
  stand where they come in. The enemies' behaviours (`Tn*CharacterBlueprint`) say how
  each moves and fights; their spawn animations, their weapons' effects and the
  deaths' gibs and particles would come with them.
- **More animations:** `Shared_ROBO_NAV_ANIM` has slides and turn-on-the-spot
  pivots. Add a clip to the `anims` list in
  `build_assets.py`, add an `ANIM_*` constant, and start it with `start_action`.
- **Tuning:** dash speed, melee reach, grenade blast size, health, the shield and
  its recharge, energon (`STARTING_ENERGON`), the health stations (`HEALTH_COST`, `HEALTH_RESPAWN_SECS`)
  and timings are constants at the top of `src\main.rs`.
- **Proper physics:** collision is hand-rolled here (`src\collision.rs`: triangle
  soup in a grid, for floors, walls and line of sight). A physics crate such as
  `avian3d` would add things like pushing objects around.
- **More maps:** `tools\build_map.py` builds an Escalation map straight from its
  packages; for another one (`ESC_*`), add its packages to its `MAPS`, any of its
  materials that need drawing specially to `SPECIAL`, and the map to `MAPS` in
  `src\main.rs` (its file name, its name and its lobby picture).
- **Reflections:** the characters reflect the game's cube maps now, and the
  garage's surfaces do; the maps' own surfaces with reflective materials could
  reflect their level's cube map the same way (levelfx.rs's surface materials).
- **More sound:** the map's ambience (`BL_LVL_*` cues) can be added to `CUES` in
  `build_sounds.py` the same way.
- **The classes' own weapons and abilities:** the Destroyer's other weapons and the
  abilities are in the same packages as the characters, and would be added as the
  Infiltrator's weapons were.
- **Faction logos:** the characters' Autobot and Decepticon logos are drawn with the
  decals' material (`TR_FactionLogo_Mat`); `build_class.py` leaves their sections out
  (`DROP`) and could keep them for `decals.rs`'s material to draw.
