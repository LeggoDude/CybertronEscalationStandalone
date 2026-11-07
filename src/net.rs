//! Online play: up to four PCs in one Escalation match. One player hosts (Host a
//! Game in the Escalation lobby) and the others join with the host's IP address
//! (Join Game). Everything goes through the host, over UDP (port 7777): it keeps
//! the lobby (who is in which of the four slots, the map, the difficulty, the
//! countdown to the match) and passes each player's packets on to the others.
//!
//! As the match starts, each player chooses his character (choose.rs); his PC says
//! when he has (`flag::CHOSEN` in his snapshots) and which class (`class`): one of
//! each class in a match, so the classes the others have are greyed out on each
//! Choose Character screen, and should two players choose the same at once, the
//! one in the lower slot keeps it and the other chooses again. Once every player in
//! the match has chosen, each a class of his own, the host starts the countdown to
//! the match proper (the starting scoreboard's, pregame.rs) and sends it with the
//! lobby: every PC counts down with it, so the match begins for all of them
//! together.
//!
//! In a match each PC plays its own Jazz and sends, 30 times a second, where he is
//! and what he is doing ([`JazzSnapshot`]) with what has happened since
//! ([`JazzEvent`]: moves started, shots, grenades, his sounds). The other PCs play
//! that out on their copy of him (main.rs). Each event is sent again in every
//! packet for a quarter of a second, so one lost packet loses nothing.
//!
//! The host can end the match (the pause menu's Return to Lobby): the roster says it
//! is over, and every PC goes back to the game's lobby, its player still in it, for
//! the host to start the next.
//!
//! A host can give the game a password (Host a Game's Game Password); a player
//! joining it then has to give it too. The password itself never crosses the
//! network: the host answers his hello with a random challenge, his PC answers
//! with the challenge signed with the password (HMAC-SHA256), and the host checks
//! the signature against its own. A wrong one is refused, and that address's
//! hellos go unanswered for a second (one guess a second at most).
//!
//! Only the standard library's sockets are used: no connection, no encryption,
//! no NAT traversal (the host's port 7777 must be reachable, as on a LAN or with
//! the port forwarded).

use bevy::prelude::*;
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};

use crate::ai::tree::{LAYERS, Layer, Pose, PoseClip};

/// The port a host listens on (UDP).
pub const DEFAULT_PORT: u16 = 7777;
/// Every packet starts with this, then its kind.
const MAGIC: [u8; 4] = *b"FOCD";
/// Changes whenever the packets do: a host turns other versions away (2: each
/// Jazz's shield; 3: the lobby's map; 4: the Nucleon Charge Rifle, its charge in
/// the snapshot's flags and its shots' charge level; 5: game passwords; 6: the Path
/// Blaster, a sixth weapon number, and the animation clips numbered after its own;
/// 7: the EMP grenade, EmpGrenade; 8: each Jazz's character look in his
/// snapshot, and the characters' clips numbered after robot.glb's; 9: the Riot
/// Cannon, a seventh weapon number, its rounds (Round), and the animation clips
/// numbered after its own; 10: the Orbital Pistol, an eighth weapon number whose
/// shots mark the sentries, and the animation clips numbered after its own; 11:
/// each look's class, the characters' clips in their class's slots; 12: the view's
/// pitch in the snapshot, for a tank's cannon, and the tank's shells as rounds; 13:
/// a Jazz not in the match yet, flag::WAITING; 14: the player's character chosen,
/// flag::CHOSEN, and the countdown to the match proper in the roster; 15: the
/// look's faction, its glow's colour, in its class byte's top bit; 16: a car's or
/// truck's steering in the snapshot, which its wheels turn and its body leans by;
/// 17: the Titan's, Scientist's and Destroyer's weapons, a gun's warm-up,
/// flag::WARMING, and a firing's first shot in its Shot's flags; 18: the Gear
/// Shredder, its blades loaded in flag::BLADES_LOW and BLADES_HIGH and a Round for
/// each blade it fires; 19: the Destroyer's weapons, a homing round's target
/// (HomingRound), a lobbed round (Lob) and rounds set off (SetOff); 20: the
/// upgrades on his weapons in the snapshot, and a special shot in a Shot's flags;
/// 21: how a Jazz died, Death, for his copies to fall the same way; 22: no frag
/// grenade, its event, 7, unused; 23: the host can end a match, Return to Lobby,
/// every player going back to its lobby when the roster says it is over; 24: the
/// class each player has chosen in his snapshot, one of each class in a match, and
/// his match so far for the scoreboard: his level in that class, score, kills,
/// deaths, revives and money; 25: the support items in use, Gear, Mine and
/// GearEnd; 26: Escalation's downed players and revives, each Jazz's downed state
/// in his snapshot (JazzSnapshot::revive), a reviver's steps (Revive) and the
/// team's lives, rounds and game over in the roster; 27: the Armory Recreators,
/// Pod and PodLoot; 28: the locked doors' unlock consoles, Deposit and Unlock; 29:
/// the class abilities, each Jazz's in his snapshot (AbilitySnap: its sentry's in
/// SentrySnap) and their moments in Ability events; 30: Prime Mode, each player's
/// in the roster, PONG and NAME; 31: the decal set in each Jazz's look; 32: the
/// sword swing's blow, Sweep; 33: Escalation's waves in the roster, WaveSnap, and
/// an enemy's death, EnemyDeath; 34: the deaths as the game's death sets play them,
/// a Death event's and an EnemyDeath's blow with its momentum and whether it gibs;
/// 35: no shooting stance hold in a JazzSnapshot, his gun always being up; 36: the
/// host's AI plays every PC's enemies, its ENEMIES packets carrying each one's pawn
/// and pose (EnemyState) and what they did (EnemyEvent), the Titans' drones in the
/// waves' roster (EnemySnap's owner), and the enemy a Shot struck by its number; 37:
/// the kill feed's deaths, KillFeed, and an enemy's blow's damage type, Hurt's; 38:
/// an AbilitySnap's meter, for the others' copies of a Cloak running low; 39: an
/// enemy's animation's effect turned off with its clip, an Effect event's
/// effect_flag::AUTO_OFF and its seconds; 40: a KillFeed's enemy by its number in
/// the waves, for each PC to show one line for its death; 41: whether a KillFeed's
/// death was a headshot).
const VERSION: u8 = 41;
const SLOTS: usize = crate::MAX_PLAYERS;
/// The longest player name kept (printable ASCII).
pub const NAME_MAX: usize = 16;
/// A player's name until he sets his own with Display Name (and an empty one's).
const DEFAULT_NAME: &str = "Bumblebee";
/// The longest game password kept (printable ASCII and spaces).
pub const PASSWORD_MAX: usize = 32;

// Packet kinds.
/// Client to host: VERSION and the player's name, until welcomed.
const HELLO: u8 = 1;
/// Host to client: the slot it has.
const WELCOME: u8 = 2;
/// Host to client: why it can't join (REFUSE_*).
const REFUSE: u8 = 3;
/// Host to clients: the lobby (see `roster_packet`).
const ROSTER: u8 = 4;
/// A Jazz's snapshot and events (from his PC to the host, which passes it on).
const STATE: u8 = 5;
/// Host to client: a token to send back (PONG, with the player's name and whether
/// he is a Prime), timing the round trip.
const PING: u8 = 6;
const PONG: u8 = 7;
/// Leaving (the host leaving ends the game for everyone).
const BYE: u8 = 8;
/// Client to host: the player's name and whether he is a Prime, changed (Display
/// Name, Prime Mode). PONG carries them too, in case this one is lost.
const NAME: u8 = 9;
/// Host to client, in answer to a hello when the game has a password: a challenge
/// (NONCE_LEN random bytes) to sign with it.
const CHALLENGE: u8 = 10;
/// Client to host: the challenge signed with the password (HMAC-SHA256, 32 bytes).
const AUTH: u8 = 11;
/// Host to clients, in a match: its enemies as its AI plays them (see
/// `enemy_packets`).
const ENEMIES: u8 = 12;

const REFUSE_VERSION: u8 = 0;
const REFUSE_FULL: u8 = 1;
const REFUSE_STARTED: u8 = 2;
/// The password was wrong.
const REFUSE_PASSWORD: u8 = 3;

const NONCE_LEN: usize = 16;
const SIGNATURE_LEN: usize = 32;
/// A host keeps a challenge this long (longer than a client says hello for:
/// CONNECT_TIMEOUT, so the one it signs is still there), and this many at once (the
/// oldest go first).
const CHALLENGE_SECS: f32 = 10.0;
const CHALLENGES_MAX: usize = 16;
/// An address that gave a wrong password goes unanswered this long.
const WRONG_PASSWORD_WAIT: f32 = 1.0;

/// A client says hello this often until the host answers, and gives up after
/// CONNECT_TIMEOUT. Either side gives the other up after TIMEOUT without a packet
/// (the game's online ConnectionTimeout, TDwNetDriver's), or MATCH_TIMEOUT in a
/// match: a PC that stops for a while (a slow load, a long hitch, its window held
/// on Windows) sends nothing meanwhile, and the others wait for it rather than
/// drop it. (A PC's own stop doesn't count against the others: its clock goes on
/// by at most a quarter of a second a frame.)
const HELLO_EVERY: f32 = 0.5;
const CONNECT_TIMEOUT: f32 = 8.0;
const TIMEOUT: f32 = 8.0;
const MATCH_TIMEOUT: f32 = 20.0;
const ROSTER_EVERY: f32 = 0.5;
const PING_EVERY: f32 = 1.0;
/// Snapshots a second, in a match.
const STATE_EVERY: f32 = 1.0 / 30.0;
/// Each event goes out in every packet for this long (and at least three times,
/// for up to a second, if the packets are late).
const EVENT_WINDOW: f32 = 0.25;
const EVENT_MAX_AGE: f32 = 1.0;
/// Bytes of events a packet carries at most (the newest first).
const EVENT_BUDGET: usize = 1000;
/// The receive buffer: a UDP datagram's most (none is cut short, which Windows
/// reports as an error).
const RECV_MAX: usize = 65536;
/// The host's enemy packets a second, in a match (each enemy in one of them).
const ENEMIES_EVERY: f32 = 1.0 / 20.0;
/// The most bytes an enemy packet carries (well under a network's frame, so it
/// isn't split on its way): the enemies go out in as many as they need.
const ENEMY_PACKET_MAX: usize = 1200;
/// Bytes of the enemies' events the first of a round of enemy packets carries at
/// most: those sent again and again for a while (as a Jazz's are), newest first,
/// and those sent once (their sounds and tracers: a lost one is soon forgotten),
/// oldest first; those sent once still waiting this long after are dropped.
const ENEMY_RESENT_BUDGET: usize = 500;
const ENEMY_ONCE_BUDGET: usize = 350;
const ENEMY_ONCE_MAX_AGE: f32 = 0.3;
/// How fast a client's idea of the host's clock eases back (seconds a second), the
/// least delayed of the host's packets setting it: should the packets take longer
/// to come, it follows.
const HOST_CLOCK_EASE: f32 = 0.05;
/// The countdown to the match after the host starts it (Lobby.MultiplayerCountdownLabel).
pub const COUNTDOWN_SECS: f32 = 5.0;
/// How far behind this PC's countdown a roster's may be without setting it back.
const COUNTDOWN_SLACK: f32 = 0.25;

/// Why online play stopped or couldn't start, for the menu to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The host didn't answer (or the address was no good).
    Connect,
    /// The host turned us away: it is full, or its match has started.
    Full,
    /// The host runs another version of the demo.
    Version,
    /// The game has a password: none was given (`wrong` false), or the one given
    /// was wrong.
    Password { wrong: bool },
    /// The host left or stopped answering.
    HostLost,
    /// Hosting failed (the port is in use...): the system's words.
    Host(String),
}

/// Where this PC is with online play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetStatus {
    Offline,
    /// Joining: waiting for the host to answer.
    Connecting,
    /// In the host's lobby (or hosting one).
    Lobby,
    /// The match has started.
    Match,
}

/// A player in the lobby.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyPlayer {
    pub name: String,
    /// The round trip to the host (0 for the host).
    pub ping_ms: u16,
    pub score: u16,
    /// He is a Prime (PrimeModeActive: his medal by his name).
    pub prime: bool,
}

/// What the PCs know of one remote Jazz: his latest snapshot (and when it came,
/// by `Net::clock`), and the events not yet played out.
#[derive(Default)]
pub struct Remote {
    pub snapshot: Option<JazzSnapshot>,
    pub received_at: f32,
    /// A snapshot came since main.rs last looked.
    pub fresh: bool,
    pub events: Vec<JazzEvent>,
    snap_seq: u32,
    last_event: Option<u32>,
}

/// Split screen's guest (split.rs): this PC's second player, in a slot of the
/// game's as a player online is; whether he has chosen his character, its class,
/// and his match so far.
#[derive(Clone, Copy, Debug, Default)]
struct GuestPlayer {
    slot: usize,
    chosen: bool,
    class: Option<u8>,
    stats: MatchStats,
}

/// One remote player as the host sees him.
struct Peer {
    addr: SocketAddr,
    slot: usize,
    last_heard: f32,
    /// The last ping sent (its token and when) and the round trip measured.
    ping: Option<(u32, f32)>,
    ping_ms: u16,
}

/// A player the host has asked for the game's password (CHALLENGE): where he is,
/// his name, the challenge and when it was made.
struct Challenge {
    addr: SocketAddr,
    name: String,
    nonce: [u8; NONCE_LEN],
    at: f32,
}

enum Role {
    Offline,
    Host {
        peers: Vec<Peer>,
    },
    Client {
        host: SocketAddr,
        welcomed: bool,
        last_hello: f32,
        started: f32,
        last_heard: f32,
    },
}

/// An event waiting to go out: its number, when it happened, the packets it has
/// gone out in, and the event (a Jazz's, or the host's enemies').
struct Outgoing<E = JazzEvent> {
    seq: u32,
    at: f32,
    sent: u8,
    event: E,
}

/// The host's enemies as the network carries them (src/ai/sync.rs plays them
/// out): hosting, the states its AI last gave and the events still going out
/// (sent again for a while, or once), the next event's number and the time to the
/// next round of packets; joined, the states and events come in (each with the
/// host's time it was from), the last event taken in, and the host's clock's
/// offset from this PC's as the least delayed packet has it.
#[derive(Default)]
struct EnemySync {
    states: Vec<EnemyState>,
    resent: VecDeque<Outgoing<EnemyEvent>>,
    once: VecDeque<(f32, EnemyEvent)>,
    next_event: u32,
    timer: f32,
    states_in: Vec<(f32, EnemyState)>,
    events_in: Vec<(f32, EnemyEvent)>,
    last_event: Option<u32>,
    host_offset: Option<f32>,
}

/// Online play (see the module docs). The menus host, join, start and leave; the
/// game hands over its own Jazz (`set_local`, `push_event`) and plays out the others
/// (`remote_mut`).
#[derive(Resource)]
pub struct Net {
    role: Role,
    socket: Option<UdpSocket>,
    /// This PC's player name, and whether he is a Prime (xp.rs's Prime Mode).
    name: String,
    prime: bool,
    /// The host's address on its network, to tell others (hosting).
    lan_ip: Option<IpAddr>,
    /// Hosting, the game's password (None: anyone can join); joining, the password
    /// to sign the host's challenge with.
    password: Option<String>,
    /// Hosting: the players asked for the password, the addresses whose password
    /// was wrong (with when they are answered again), and the challenges made so
    /// far.
    challenges: Vec<Challenge>,
    refused: Vec<(IpAddr, f32)>,
    challenge_count: u64,
    players: [Option<LobbyPlayer>; SLOTS],
    me: Option<usize>,
    /// Split screen's guest, while there is one (hosting: none can join online
    /// meanwhile).
    guest: Option<GuestPlayer>,
    /// Difficulty Setting: 0 easy, 1 normal, 2 hard (the host's choice).
    difficulty: u8,
    /// Select Map: the map to play (an index into main.rs's MAPS; the host's choice).
    map: u8,
    /// Seconds left of the countdown to the match.
    countdown: Option<f32>,
    in_match: bool,
    /// In a match: whether this PC's player has chosen his character, the class
    /// of the one he has chosen (he may choose another from the pause menu), and
    /// the seconds left of the countdown that starts once every player has (the
    /// host's; 0 once the match has begun).
    chosen_here: bool,
    class_here: Option<u8>,
    /// This PC's player's match so far, as the scoreboard shows it (scoreboard.rs).
    stats_here: MatchStats,
    match_countdown: Option<f32>,
    /// The team's lives left, failed rounds and game over (revive.rs's Team): the
    /// host's, as it sets them and its roster carries them (a client's, as the last
    /// roster said; None before one has).
    team: Option<(u8, u8, bool)>,
    /// Escalation's waves (waves.rs): the host's (or a lone player's), as its roster
    /// carries them; a client's, as the newest roster said (None before one has).
    waves: Option<WaveSnap>,
    failure: Option<Failure>,
    /// Seconds since the demo started (real time).
    clock: f32,
    remote: [Remote; SLOTS],
    local: Option<JazzSnapshot>,
    outbox: VecDeque<Outgoing>,
    next_event: u32,
    snap_seq: u32,
    state_timer: f32,
    roster_timer: f32,
    ping_timer: f32,
    roster_dirty: bool,
    /// The enemies, as the host's AI plays them, and the clock they are timed by
    /// (real time from here: unlike `clock`, never held back by a slow frame, which
    /// would put the host's clock out of step with its players' idea of it).
    enemies: EnemySync,
    epoch: std::time::Instant,
    /// The long frames the net log has had this game (LONG_FRAMES_LOGGED at most).
    long_frames: u32,
}

impl Default for Net {
    fn default() -> Self {
        Net {
            role: Role::Offline,
            socket: None,
            name: DEFAULT_NAME.to_string(),
            prime: false,
            lan_ip: None,
            password: None,
            challenges: Vec::new(),
            refused: Vec::new(),
            challenge_count: 0,
            players: Default::default(),
            me: None,
            guest: None,
            difficulty: 1,
            map: crate::built_maps().first().copied().unwrap_or(0) as u8,
            countdown: None,
            in_match: false,
            chosen_here: false,
            class_here: None,
            stats_here: MatchStats::default(),
            match_countdown: None,
            team: None,
            waves: None,
            failure: None,
            clock: 0.0,
            remote: Default::default(),
            local: None,
            outbox: VecDeque::new(),
            next_event: 0,
            snap_seq: 0,
            state_timer: 0.0,
            roster_timer: 0.0,
            ping_timer: 0.0,
            roster_dirty: false,
            enemies: EnemySync::default(),
            epoch: std::time::Instant::now(),
            long_frames: 0,
        }
    }
}

impl Drop for Net {
    /// Quitting the demo leaves the game properly.
    fn drop(&mut self) {
        self.leave();
    }
}

/// A name fit to show: printable ASCII, at most NAME_MAX characters.
fn clean_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(NAME_MAX)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        DEFAULT_NAME.to_string()
    } else {
        name.to_string()
    }
}

/// A game password as it is kept and compared: printable ASCII and spaces, at most
/// PASSWORD_MAX characters, without spaces at its ends (None: no password).
pub fn clean_password(password: &str) -> Option<String> {
    let password: String = password
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(PASSWORD_MAX)
        .collect();
    let password = password.trim();
    (!password.is_empty()).then(|| password.to_string())
}

/// An address typed in: an IP address or host name, with or without a port.
fn parse_address(text: &str) -> Option<SocketAddr> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(addr) = text.parse::<SocketAddr>() {
        return Some(addr);
    }
    if let Ok(ip) = text.parse::<IpAddr>() {
        return Some(SocketAddr::new(ip, DEFAULT_PORT));
    }
    // A host name: with its port, or the default.
    let found: Vec<SocketAddr> = match text.rsplit_once(':') {
        Some((name, port)) if port.parse::<u16>().is_ok() => (name, port.parse::<u16>().ok()?)
            .to_socket_addrs()
            .ok()?
            .collect(),
        _ => (text, DEFAULT_PORT).to_socket_addrs().ok()?.collect(),
    };
    found
        .iter()
        .find(|a| a.is_ipv4())
        .or(found.first())
        .copied()
}

/// This PC's address on its network (the one its way out uses; nothing is sent).
fn lan_ip() -> Option<IpAddr> {
    let probe = UdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect("8.8.8.8:80").ok()?;
    let ip = probe.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

impl Net {
    /// This PC's player name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this PC's player is a Prime.
    pub fn prime(&self) -> bool {
        self.prime
    }

    /// Prime Mode (xp.rs's progress): this PC's player is a Prime or not, here and
    /// for the others (as a name changes: set_name).
    pub fn set_prime(&mut self, prime: bool) {
        if prime == self.prime {
            return;
        }
        self.prime = prime;
        if let Some(Some(p)) = self.me.map(|slot| &mut self.players[slot]) {
            p.prime = prime;
        }
        match &self.role {
            Role::Host { .. } => self.roster_dirty = true,
            Role::Client {
                host,
                welcomed: true,
                ..
            } => {
                let host = *host;
                let mut w = Writer::new(NAME);
                w.str8(&self.name);
                w.u8(u8::from(self.prime));
                self.send_to(host, &w.0);
            }
            _ => {}
        }
    }

    /// Display Name: this PC's player name changes, here and (in a game's lobby
    /// or match) for the others: the host sends them the roster again, a client
    /// tells the host.
    pub fn set_name(&mut self, name: &str) {
        let name = clean_name(name);
        if name == self.name {
            return;
        }
        self.name = name;
        if let Some(Some(p)) = self.me.map(|slot| &mut self.players[slot]) {
            p.name = self.name.clone();
        }
        match &self.role {
            Role::Host { .. } => self.roster_dirty = true,
            Role::Client {
                host,
                welcomed: true,
                ..
            } => {
                let host = *host;
                let mut w = Writer::new(NAME);
                w.str8(&self.name);
                w.u8(u8::from(self.prime));
                self.send_to(host, &w.0);
            }
            _ => {}
        }
    }

    /// Hosts a game: listens on DEFAULT_PORT, with this player in slot 0. With a
    /// password (not empty), only the players who give it can join.
    pub fn host(&mut self, password: &str) -> Result<(), Failure> {
        self.leave();
        self.failure = None;
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)))
            .and_then(|s| s.set_nonblocking(true).map(|_| s))
            .map_err(|e| Failure::Host(e.to_string()))?;
        self.socket = Some(socket);
        self.role = Role::Host { peers: Vec::new() };
        self.password = clean_password(password);
        self.me = Some(0);
        self.players[0] = Some(LobbyPlayer {
            name: self.name.clone(),
            ping_ms: 0,
            score: 0,
            prime: self.prime,
        });
        self.lan_ip = lan_ip();
        // (The map last seen in someone else's lobby may not have been built here.)
        let built = crate::built_maps();
        if !built.contains(&self.map()) {
            self.map = built.first().copied().unwrap_or(0) as u8;
        }
        self.roster_dirty = true;
        self.long_frames = 0;
        self.note(&format!("hosting a game on port {DEFAULT_PORT}"));
        Ok(())
    }

    /// Offline Game (the party lobby's): a game of this PC's own, hosted with no
    /// network at all (no port opened, so no one online can join it, and no
    /// password to ask for): this player in slot 0, and split screen's guest
    /// (split.rs) in the next, should he join.
    pub fn host_offline(&mut self) {
        self.leave();
        self.failure = None;
        self.role = Role::Host { peers: Vec::new() };
        self.password = None;
        self.me = Some(0);
        self.players[0] = Some(LobbyPlayer {
            name: self.name.clone(),
            ping_ms: 0,
            score: 0,
            prime: self.prime,
        });
        let built = crate::built_maps();
        if !built.contains(&self.map()) {
            self.map = built.first().copied().unwrap_or(0) as u8;
        }
        self.roster_dirty = true;
        self.long_frames = 0;
    }

    /// Hosting an Offline Game (host_offline: no network).
    pub fn is_offline_game(&self) -> bool {
        self.is_host() && self.socket.is_none()
    }

    /// Joins the game hosted at `address` (an IP address or host name, with or
    /// without a port) with `password` (empty: none), should the game have one:
    /// says hello until the host answers (NetStatus::Connecting). A game with a
    /// password stops it with Failure::Password when none was given, or the wrong
    /// one.
    pub fn join(&mut self, address: &str, password: &str) -> Result<(), Failure> {
        self.leave();
        self.failure = None;
        let host = parse_address(address).ok_or(Failure::Connect)?;
        let any = if host.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = UdpSocket::bind(any)
            .and_then(|s| s.set_nonblocking(true).map(|_| s))
            .map_err(|_| Failure::Connect)?;
        self.socket = Some(socket);
        self.role = Role::Client {
            host,
            welcomed: false,
            last_hello: f32::NEG_INFINITY,
            started: self.clock,
            last_heard: self.clock,
        };
        self.password = clean_password(password);
        self.long_frames = 0;
        self.note(&format!("joining the game at {host}"));
        Ok(())
    }

    /// Leaves the game (the host ends it for everyone). Keeps any failure for the
    /// menu to show.
    pub fn leave(&mut self) {
        let mut w = Writer::new(BYE);
        w.u8(0);
        if !matches!(self.role, Role::Offline) {
            self.note("left the game");
        }
        match &self.role {
            Role::Host { peers } => {
                for peer in peers {
                    for _ in 0..3 {
                        self.send_to(peer.addr, &w.0);
                    }
                }
            }
            Role::Client { host, .. } => {
                for _ in 0..3 {
                    self.send_to(*host, &w.0);
                }
            }
            Role::Offline => {}
        }
        self.go_offline();
    }

    fn go_offline(&mut self) {
        self.role = Role::Offline;
        self.socket = None;
        self.players = Default::default();
        self.me = None;
        self.guest = None;
        self.countdown = None;
        self.in_match = false;
        self.lan_ip = None;
        self.password = None;
        self.challenges.clear();
        self.refused.clear();
        self.reset_match();
    }

    /// Forgets the remote Jazzes and this one's events (a match starting), and
    /// who has chosen his character.
    pub fn reset_match(&mut self) {
        self.remote = Default::default();
        self.local = None;
        self.outbox.clear();
        self.next_event = 0;
        self.snap_seq = 0;
        self.chosen_here = false;
        self.class_here = None;
        self.stats_here = MatchStats::default();
        if let Some(g) = &mut self.guest {
            *g = GuestPlayer {
                slot: g.slot,
                ..GuestPlayer::default()
            };
        }
        self.match_countdown = None;
        self.team = None;
        self.waves = None;
        self.enemies = EnemySync::default();
    }

    /// The team's lives left, failed rounds and whether the game is over, as the
    /// host has them (None before a roster has said).
    pub fn team(&self) -> Option<(u8, u8, bool)> {
        self.team
    }

    /// The host's (or a lone player's) count of the team, for its rosters.
    pub fn set_team(&mut self, lives: u8, round: u8, over: bool) {
        self.team = Some((lives, round, over));
    }

    /// Escalation's waves as the host has them (a client's: the newest roster's).
    pub fn waves(&self) -> Option<&WaveSnap> {
        self.waves.as_ref()
    }

    /// The host's (or a lone player's) waves (waves.rs), for its rosters: a change
    /// is numbered and goes out at once.
    pub fn set_waves(&mut self, mut snap: WaveSnap) {
        match &self.waves {
            Some(old) if old.same(&snap) => return,
            Some(old) => snap.seq = old.seq.wrapping_add(1),
            None => snap.seq = 1,
        }
        self.waves = Some(snap);
        self.roster_dirty = true;
    }

    /// A match's waves start over (waves.rs, as a match starts).
    pub fn clear_waves(&mut self) {
        self.waves = None;
    }

    pub fn status(&self) -> NetStatus {
        match &self.role {
            Role::Offline => NetStatus::Offline,
            Role::Client {
                welcomed: false, ..
            } => NetStatus::Connecting,
            _ if self.in_match => NetStatus::Match,
            _ => NetStatus::Lobby,
        }
    }

    pub fn is_host(&self) -> bool {
        matches!(self.role, Role::Host { .. })
    }

    /// Hosting a game with a password. (The lobby's ticker showed this and
    /// `host_address`; it shows a welcome now.)
    #[allow(dead_code)]
    pub fn has_password(&self) -> bool {
        self.is_host() && self.password.is_some()
    }

    /// This PC's slot (0 hosting; 1-3 joined).
    pub fn my_slot(&self) -> Option<usize> {
        self.me
    }

    /// The players in the lobby, by slot.
    pub fn players(&self) -> impl Iterator<Item = (usize, &LobbyPlayer)> {
        self.players
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().map(|p| (i, p)))
    }

    pub fn player_count(&self) -> usize {
        self.players.iter().flatten().count()
    }

    /// Difficulty Setting: 0 easy, 1 normal, 2 hard.
    pub fn difficulty(&self) -> usize {
        self.difficulty as usize
    }

    /// The host changes the difficulty.
    pub fn set_difficulty(&mut self, difficulty: usize) {
        if self.is_host() && !self.in_match {
            self.difficulty = difficulty.min(2) as u8;
            self.roster_dirty = true;
        }
    }

    /// Select Map: the map to play (an index into MAPS).
    pub fn map(&self) -> usize {
        self.map as usize
    }

    /// The host picks the map.
    pub fn set_map(&mut self, map: usize) {
        if self.is_host() && !self.in_match && map < crate::MAPS.len() {
            self.map = map as u8;
            self.roster_dirty = true;
        }
    }

    /// The host starts the countdown to the match.
    pub fn start_countdown(&mut self) {
        if self.is_host() && !self.in_match && self.countdown.is_none() {
            self.countdown = Some(COUNTDOWN_SECS);
            self.roster_dirty = true;
        }
    }

    /// Return to Lobby: the host ends the match, and every player goes back to the
    /// game's lobby, still in the game (the roster says the match is over).
    pub fn end_match(&mut self) {
        if self.is_host() && self.in_match {
            self.in_match = false;
            self.countdown = None;
            self.roster_dirty = true;
            self.reset_match();
        }
    }

    /// Seconds left before the match starts, while counting down.
    pub fn countdown(&self) -> Option<f32> {
        self.countdown
    }

    pub fn in_match(&self) -> bool {
        self.in_match
    }

    /// This PC's player has chosen his character (or not yet), as the match starts.
    pub fn set_chosen(&mut self, chosen: bool) {
        self.chosen_here = chosen;
    }

    /// The class of the character this PC's player has chosen (None: none yet).
    pub fn set_class(&mut self, class: Option<usize>) {
        self.class_here = class.map(|c| c.min(u8::MAX as usize - 1) as u8);
    }

    /// The class of the character the player in `slot` has chosen, if he has.
    fn class_of(&self, slot: usize) -> Option<u8> {
        if Some(slot) == self.me {
            return self.class_here;
        }
        if let Some(g) = self.guest.filter(|g| g.slot == slot) {
            return g.class;
        }
        self.remote[slot]
            .snapshot
            .map(|s| s.class)
            .filter(|&c| c != u8::MAX)
    }

    /// Who has `class` in the match: of the players who have chosen it, the one in
    /// the lowest slot (should two choose it at once, he keeps it).
    fn holder(&self, class: u8) -> Option<usize> {
        self.players()
            .map(|(slot, _)| slot)
            .find(|&slot| self.class_of(slot) == Some(class))
    }

    /// The classes another player in the match has (greyed out on this PC's Choose
    /// Character screen), each with his name. None offline or outside a match.
    pub fn classes_taken(&self) -> Vec<(usize, String)> {
        match self.me {
            Some(me) => self.classes_taken_from(me),
            None => Vec::new(),
        }
    }

    /// The same for the player in slot `mine` (split screen's guest's Choose
    /// Character screen: the first player's class among them).
    pub fn classes_taken_from(&self, mine: usize) -> Vec<(usize, String)> {
        if !self.in_match {
            return Vec::new();
        }
        (0..crate::character::CLASSES.len() as u8)
            .filter_map(|class| {
                let slot = self.holder(class)?;
                (slot != mine).then(|| {
                    let name = self.players[slot].as_ref().map(|p| p.name.clone());
                    (class as usize, name.unwrap_or_default())
                })
            })
            .collect()
    }

    /// This PC's player's match so far (scoreboard.rs), for the others' scoreboards.
    pub fn set_stats(&mut self, stats: MatchStats) {
        self.stats_here = stats;
    }

    /// The players in the match as the scoreboard lists them: each one's slot, name,
    /// whether he is this PC's, the class of his character (as he has chosen it),
    /// his match so far and his connection's round trip. Empty outside a match.
    pub fn scoreboard(&self) -> Vec<ScoreLine> {
        if !self.in_match {
            return Vec::new();
        }
        self.players()
            .map(|(slot, p)| {
                let me = Some(slot) == self.me;
                let guest = self.guest.filter(|g| g.slot == slot);
                let stats = if me {
                    self.stats_here
                } else if let Some(g) = guest {
                    g.stats
                } else {
                    self.remote[slot]
                        .snapshot
                        .map(|s| s.stats)
                        .unwrap_or_default()
                };
                ScoreLine {
                    slot,
                    name: p.name.clone(),
                    // (Both of this PC's players.)
                    me: me || guest.is_some(),
                    class: self.class_of(slot).map(usize::from),
                    stats,
                    ping_ms: p.ping_ms,
                }
            })
            .collect()
    }

    /// The class this PC's player chose went to another player, who chose it at
    /// the same time from a lower slot: he chooses again.
    pub fn class_lost(&self) -> bool {
        self.in_match
            && self
                .class_here
                .is_some_and(|class| self.holder(class).is_some_and(|slot| Some(slot) != self.me))
    }

    /// Split screen's guest's slot, while he is in.
    pub fn guest_slot(&self) -> Option<usize> {
        self.guest.map(|g| g.slot)
    }

    /// Split screen's guest joins this PC's game (split.rs), as `name`, in its first
    /// free slot: hosting, in the lobby (not counting down) with nobody online in
    /// it. His slot, or None if he can't.
    pub fn add_guest(&mut self, name: &str) -> Option<usize> {
        if !self.is_host()
            || self.in_match
            || self.countdown.is_some()
            || self.guest.is_some()
            || self.player_count() != 1
        {
            return None;
        }
        let slot = (1..SLOTS).find(|s| self.players[*s].is_none())?;
        self.players[slot] = Some(LobbyPlayer {
            name: clean_name(name),
            ping_ms: 0,
            score: 0,
            prime: false,
        });
        self.remote[slot] = Remote::default();
        self.guest = Some(GuestPlayer {
            slot,
            ..GuestPlayer::default()
        });
        self.roster_dirty = true;
        self.note(&format!("split screen: {name} joined, in slot {slot}"));
        Some(slot)
    }

    /// Split screen's guest leaves (in the lobby).
    pub fn remove_guest(&mut self) {
        if self.in_match {
            return;
        }
        if let Some(g) = self.guest.take() {
            self.players[g.slot] = None;
            self.remote[g.slot] = Remote::default();
            self.roster_dirty = true;
            self.note("split screen: the second player left");
        }
    }

    /// Split screen's guest has chosen his character (or not yet).
    pub fn set_guest_chosen(&mut self, chosen: bool) {
        if let Some(g) = &mut self.guest {
            g.chosen = chosen;
        }
    }

    /// The class of the character split screen's guest has chosen.
    pub fn set_guest_class(&mut self, class: Option<usize>) {
        if let Some(g) = &mut self.guest {
            g.class = class.map(|c| c.min(u8::MAX as usize - 1) as u8);
        }
    }

    /// Split screen's guest's match so far (scoreboard.rs).
    pub fn set_guest_stats(&mut self, stats: MatchStats) {
        if let Some(g) = &mut self.guest {
            g.stats = stats;
        }
    }

    /// Seconds left before the match proper begins, once every player has chosen
    /// his character (0 from then on); None until then.
    pub fn match_countdown(&self) -> Option<f32> {
        self.match_countdown.filter(|_| self.in_match)
    }

    /// The host: every player in the match has chosen his character (this PC's
    /// player by `set_chosen`, the others by their snapshots), each a class no one
    /// else has.
    fn all_chosen(&self) -> bool {
        self.players().all(|(slot, _)| {
            let guest = self.guest.filter(|g| g.slot == slot);
            let chosen = if Some(slot) == self.me {
                self.chosen_here
            } else if let Some(g) = guest {
                g.chosen
            } else {
                self.remote[slot]
                    .snapshot
                    .is_some_and(|s| s.has(flag::CHOSEN))
            };
            chosen
                && self
                    .class_of(slot)
                    .is_none_or(|class| self.holder(class) == Some(slot))
        })
    }

    /// Why online play stopped, once.
    pub fn take_failure(&mut self) -> Option<Failure> {
        self.failure.take()
    }

    /// Online play has stopped, and the menu hasn't yet said why (take_failure).
    pub fn failed(&self) -> bool {
        self.failure.is_some()
    }

    /// Where others can join this host: its address on its network and the port.
    #[allow(dead_code)]
    pub fn host_address(&self) -> Option<String> {
        if !self.is_host() {
            return None;
        }
        let ip = self
            .lan_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        Some(format!("{ip}:{DEFAULT_PORT}"))
    }

    /// Seconds since the demo started (real time), as `Remote::received_at` has it.
    pub fn clock(&self) -> f32 {
        self.clock
    }

    /// This PC's Jazz as he is now (sent with the next packet).
    pub fn set_local(&mut self, snapshot: JazzSnapshot) {
        self.local = Some(snapshot);
    }

    /// Something this PC's Jazz did, for the others to play out.
    pub fn push_event(&mut self, event: JazzEvent) {
        self.outbox.push_back(Outgoing {
            seq: self.next_event,
            at: self.clock,
            sent: 0,
            event,
        });
        self.next_event = self.next_event.wrapping_add(1);
    }

    /// The Jazz of player `slot`, as his PC last sent him.
    pub fn remote_mut(&mut self, slot: usize) -> Option<&mut Remote> {
        self.remote.get_mut(slot)
    }

    /// Hosting: its enemies as its AI has them this frame (src/ai/sync.rs), for the
    /// next round of enemy packets.
    pub fn set_enemy_states(&mut self, states: Vec<EnemyState>) {
        self.enemies.states = states;
    }

    /// Hosting: something its enemies did this frame, for the others' copies of
    /// them to do too: sent again for a while (`resent`, as a Jazz's events are),
    /// or once.
    pub fn push_enemy_event(&mut self, event: EnemyEvent, resent: bool) {
        let now = self.sync_clock();
        let e = &mut self.enemies;
        if resent {
            e.resent.push_back(Outgoing {
                seq: e.next_event,
                at: now,
                sent: 0,
                event,
            });
            e.next_event = e.next_event.wrapping_add(1);
        } else {
            e.once.push_back((now, event));
        }
    }

    /// The clock the host's enemies are timed by (seconds of real time).
    pub fn sync_clock(&self) -> f32 {
        self.epoch.elapsed().as_secs_f32()
    }

    /// Joined: the host's enemy states come in since last asked, each with the
    /// host's time it was from (by its clock).
    pub fn take_enemy_states(&mut self) -> Vec<(f32, EnemyState)> {
        std::mem::take(&mut self.enemies.states_in)
    }

    /// Joined: the host's enemies' events come in since last asked, each once, with
    /// the host's time it happened.
    pub fn take_enemy_events(&mut self) -> Vec<(f32, EnemyEvent)> {
        std::mem::take(&mut self.enemies.events_in)
    }

    /// Joined: the host's clock now (its `sync_clock`), as its least delayed packet
    /// has it (the time of its newest states as they come in); None before its
    /// first enemy packet.
    pub fn host_clock(&self) -> Option<f32> {
        self.enemies.host_offset.map(|o| self.sync_clock() + o)
    }

    fn send_to(&self, to: SocketAddr, data: &[u8]) {
        if let Some(socket) = &self.socket {
            // (Nothing to do about a failed send: UDP may lose packets anyway.)
            let _ = socket.send_to(data, to);
        }
    }

    // --- receiving -----------------------------------------------------------

    fn receive(&mut self) {
        let mut packets = Vec::new();
        if let Some(socket) = &self.socket {
            let mut buf = vec![0u8; RECV_MAX];
            // (A bounded number a frame, so a flood can't stall the game.)
            for _ in 0..512 {
                match socket.recv_from(&mut buf) {
                    Ok((n, from)) => packets.push((buf[..n].to_vec(), from)),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    // Any other error is about one packet (an earlier send that
                    // reached a closed port, which Windows and Linux report on the
                    // next receive; one cut short): on to the next, those behind
                    // it not left waiting for the next frame.
                    Err(_) => continue,
                }
            }
        }
        for (data, from) in packets {
            if data.len() < 5 || data[..4] != MAGIC {
                continue;
            }
            if self.is_host() {
                self.host_packet(&data, from);
            } else {
                self.client_packet(&data, from);
            }
        }
    }

    fn peer_index(&self, from: SocketAddr) -> Option<usize> {
        match &self.role {
            Role::Host { peers } => peers.iter().position(|p| p.addr == from),
            _ => None,
        }
    }

    fn host_packet(&mut self, data: &[u8], from: SocketAddr) {
        let clock = self.clock;
        let peer = self.peer_index(from);
        let mut silence = 0.0;
        let peer_slot = peer.and_then(|i| match &mut self.role {
            Role::Host { peers } => {
                silence = clock - peers[i].last_heard;
                peers[i].last_heard = clock;
                Some(peers[i].slot)
            }
            _ => None,
        });
        if let Some(slot) = peer_slot
            && silence > LONG_SILENCE
        {
            let who = self.player_name(slot);
            self.note(&format!(
                "nothing came from {who} for {silence:.1} s, then he was heard again"
            ));
        }
        let mut r = Reader::new(&data[5..]);
        match data[4] {
            HELLO => {
                if let Some(slot) = peer_slot {
                    // (Our welcome was lost.)
                    self.welcome(from, slot);
                    return;
                }
                // (A wrong password a moment ago: not yet.)
                if self
                    .refused
                    .iter()
                    .any(|&(ip, until)| ip == from.ip() && clock < until)
                {
                    return;
                }
                let version = r.u8();
                let name = clean_name(&r.str8().unwrap_or_default());
                if version != Some(VERSION) {
                    self.refuse(from, REFUSE_VERSION);
                    return;
                }
                if let Some(reason) = self.cannot_join() {
                    self.refuse(from, reason);
                    return;
                }
                if self.password.is_some() {
                    self.challenge(from, name);
                } else {
                    self.admit(from, name);
                }
            }
            AUTH => {
                if let Some(slot) = peer_slot {
                    self.welcome(from, slot);
                    return;
                }
                let (Some(i), Some(signature)) = (
                    self.challenges.iter().position(|c| c.addr == from),
                    r.take(SIGNATURE_LEN),
                ) else {
                    return;
                };
                let challenge = self.challenges.remove(i);
                let right = self.password.as_deref().is_some_and(|password| {
                    same_bytes(
                        signature,
                        &hmac_sha256(password.as_bytes(), &challenge.nonce),
                    )
                });
                if !right {
                    self.refused.push((from.ip(), clock + WRONG_PASSWORD_WAIT));
                    self.refuse(from, REFUSE_PASSWORD);
                } else if let Some(reason) = self.cannot_join() {
                    self.refuse(from, reason);
                } else {
                    self.admit(from, challenge.name);
                }
            }
            // (From a player no longer in the game, given up for silence while he
            // thought himself in it: told so, at once.)
            STATE | PONG | NAME if peer_slot.is_none() => {
                let mut w = Writer::new(BYE);
                w.u8(0);
                self.send_to(from, &w.0);
            }
            STATE => {
                let Some(slot) = peer_slot else {
                    return;
                };
                // On to the others, as from his slot.
                let mut relay = data.to_vec();
                if relay.len() > 5 {
                    relay[5] = slot as u8;
                }
                if let Role::Host { peers } = &self.role {
                    for p in peers.iter().filter(|p| p.slot != slot) {
                        self.send_to(p.addr, &relay);
                    }
                }
                let _ = r.u8();
                self.read_state(slot, &mut r);
            }
            PONG => {
                let (Some(i), Some(token)) = (peer, r.u32()) else {
                    return;
                };
                if let Role::Host { peers } = &mut self.role
                    && let Some((sent_token, at)) = peers[i].ping
                    && sent_token == token
                {
                    peers[i].ping = None;
                    peers[i].ping_ms = ((clock - at) * 1000.0).clamp(0.0, 9999.0) as u16;
                }
                if let (Some(slot), Some(name)) = (peer_slot, r.str8()) {
                    self.rename(slot, &name, r.u8() == Some(1));
                }
            }
            NAME => {
                if let (Some(slot), Some(name)) = (peer_slot, r.str8()) {
                    self.rename(slot, &name, r.u8() == Some(1));
                }
            }
            BYE => {
                if let Some(slot) = peer_slot {
                    let who = self.player_name(slot);
                    self.note(&format!("{who} left the game"));
                    self.drop_peer(slot);
                }
            }
            _ => {}
        }
    }

    /// Why no one can join now: the match has started, or every slot is taken (or
    /// this PC has split screen's guest: the game is this PC's alone meanwhile).
    fn cannot_join(&self) -> Option<u8> {
        if self.in_match {
            Some(REFUSE_STARTED)
        } else if self.guest.is_some() || (1..SLOTS).all(|slot| self.players[slot].is_some()) {
            Some(REFUSE_FULL)
        } else {
            None
        }
    }

    fn refuse(&self, to: SocketAddr, reason: u8) {
        let mut w = Writer::new(REFUSE);
        w.u8(reason);
        self.send_to(to, &w.0);
    }

    fn welcome(&self, to: SocketAddr, slot: usize) {
        let mut w = Writer::new(WELCOME);
        w.u8(slot as u8);
        self.send_to(to, &w.0);
    }

    /// The player at `from` joins the lobby, in the first free slot.
    fn admit(&mut self, from: SocketAddr, name: String) {
        let Some(slot) = (1..SLOTS).find(|s| self.players[*s].is_none()) else {
            self.refuse(from, REFUSE_FULL);
            return;
        };
        self.players[slot] = Some(LobbyPlayer {
            name,
            ping_ms: 0,
            score: 0,
            prime: false,
        });
        self.remote[slot] = Remote::default();
        let clock = self.clock;
        if let Role::Host { peers } = &mut self.role {
            peers.push(Peer {
                addr: from,
                slot,
                last_heard: clock,
                ping: None,
                ping_ms: 0,
            });
        }
        self.welcome(from, slot);
        self.roster_dirty = true;
        let who = self.player_name(slot);
        self.note(&format!("{who} joined, from {from}"));
    }

    /// Asks the player at `from` for the game's password: a challenge to sign with
    /// it (the same one each time he says hello, until it is answered or too old).
    fn challenge(&mut self, from: SocketAddr, name: String) {
        let nonce = match self.challenges.iter_mut().find(|c| c.addr == from) {
            Some(c) => {
                c.name = name;
                c.nonce
            }
            None => {
                if self.challenges.len() >= CHALLENGES_MAX {
                    self.challenges.remove(0);
                }
                let nonce = self.new_nonce(from);
                self.challenges.push(Challenge {
                    addr: from,
                    name,
                    nonce,
                    at: self.clock,
                });
                nonce
            }
        };
        let mut w = Writer::new(CHALLENGE);
        w.0.extend_from_slice(&nonce);
        self.send_to(from, &w.0);
    }

    /// NONCE_LEN bytes no one can foresee, for a challenge: from the standard
    /// library's hasher, which every RandomState keys afresh from the system's
    /// random numbers.
    fn new_nonce(&mut self, from: SocketAddr) -> [u8; NONCE_LEN] {
        self.challenge_count += 1;
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let mut nonce = [0u8; NONCE_LEN];
        for (i, part) in nonce.chunks_mut(8).enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(self.challenge_count);
            h.write_usize(i);
            h.write_u128(since_epoch);
            from.hash(&mut h);
            part.copy_from_slice(&h.finish().to_le_bytes());
        }
        nonce
    }

    /// A player has a new name: the others get it with the next roster.
    fn rename(&mut self, slot: usize, name: &str, prime: bool) {
        let name = clean_name(name);
        if let Some(Some(p)) = self.players.get_mut(slot)
            && (p.name != name || p.prime != prime)
        {
            p.name = name;
            p.prime = prime;
            self.roster_dirty = true;
        }
    }

    /// A player has left (or stopped answering): his slot is free again.
    fn drop_peer(&mut self, slot: usize) {
        if let Role::Host { peers } = &mut self.role {
            peers.retain(|p| p.slot != slot);
        }
        self.players[slot] = None;
        self.remote[slot] = Remote::default();
        self.roster_dirty = true;
    }

    fn client_packet(&mut self, data: &[u8], from: SocketAddr) {
        let clock = self.clock;
        if let Role::Client {
            host,
            welcomed: true,
            last_heard,
            ..
        } = &self.role
            && from == *host
            && clock - *last_heard > LONG_SILENCE
        {
            let silence = clock - *last_heard;
            self.note(&format!(
                "nothing came from the host for {silence:.1} s, then it was heard again"
            ));
        }
        let Role::Client {
            host,
            welcomed,
            last_heard,
            ..
        } = &mut self.role
        else {
            return;
        };
        if from != *host {
            return;
        }
        *last_heard = clock;
        let host = *host;
        let mut r = Reader::new(&data[5..]);
        match data[4] {
            WELCOME => {
                if let (false, Some(slot)) = (*welcomed, r.u8()) {
                    if (slot as usize) < SLOTS {
                        *welcomed = true;
                        self.me = Some(slot as usize);
                    }
                }
            }
            REFUSE => {
                if *welcomed {
                    return;
                }
                self.failure = Some(match r.u8() {
                    Some(REFUSE_VERSION) => Failure::Version,
                    Some(REFUSE_PASSWORD) => Failure::Password { wrong: true },
                    _ => Failure::Full,
                });
                self.go_offline();
            }
            CHALLENGE => {
                let Some(nonce) = r.take(NONCE_LEN) else {
                    return;
                };
                if *welcomed {
                    return;
                }
                // The game has a password: sign the challenge with ours, or stop
                // for the player to give it.
                match &self.password {
                    Some(password) => {
                        let mut w = Writer::new(AUTH);
                        w.0.extend_from_slice(&hmac_sha256(password.as_bytes(), nonce));
                        self.send_to(host, &w.0);
                    }
                    None => {
                        self.failure = Some(Failure::Password { wrong: false });
                        self.go_offline();
                    }
                }
            }
            ROSTER => {
                if *welcomed {
                    self.read_roster(&mut r);
                }
            }
            STATE => {
                if let Some(slot) = r.u8()
                    && (slot as usize) < SLOTS
                    && Some(slot as usize) != self.me
                {
                    self.read_state(slot as usize, &mut r);
                }
            }
            ENEMIES => {
                if *welcomed && self.in_match {
                    self.read_enemies(&mut r);
                }
            }
            PING => {
                if let Some(token) = r.u32() {
                    let mut w = Writer::new(PONG);
                    w.u32(token);
                    w.str8(&self.name);
                    w.u8(u8::from(self.prime));
                    self.send_to(host, &w.0);
                }
            }
            BYE => {
                self.note("the host ended the game for this PC (it left, or had given this PC up)");
                self.failure = Some(Failure::HostLost);
                self.go_offline();
            }
            _ => {}
        }
    }

    fn read_roster(&mut self, r: &mut Reader) {
        let (
            Some(flags),
            Some(countdown),
            Some(difficulty),
            Some(map),
            Some(match_countdown),
            Some(count),
        ) = (r.u8(), r.u16(), r.u8(), r.u8(), r.u16(), r.u8())
        else {
            return;
        };
        let mut players: [Option<LobbyPlayer>; SLOTS] = Default::default();
        for _ in 0..count {
            let (Some(slot), Some(ping_ms), Some(score), Some(name), Some(prime)) =
                (r.u8(), r.u16(), r.u16(), r.str8(), r.u8())
            else {
                return;
            };
            if let Some(entry) = players.get_mut(slot as usize) {
                *entry = Some(LobbyPlayer {
                    name: clean_name(&name),
                    ping_ms,
                    score,
                    prime: prime != 0,
                });
            }
        }
        // A slot that emptied or filled forgets its Jazz (a new name is the same
        // player renamed: Display Name).
        for (slot, entry) in players.iter().enumerate() {
            if entry.is_some() != self.players[slot].is_some() {
                self.remote[slot] = Remote::default();
            }
        }
        self.players = players;
        if let (Some(lives), Some(round), Some(over)) = (r.u8(), r.u8(), r.u8()) {
            self.team = (lives != u8::MAX).then_some((lives, round, over != 0));
        }
        let waves = (r.u8() == Some(1)).then(|| WaveSnap::read(r)).flatten();
        let in_match = flags & 1 != 0;
        let starts = in_match && !self.in_match;
        if starts {
            self.reset_match();
        }
        // (Newer than the last: a roster that arrives late changes nothing.)
        if let Some(snap) = waves.filter(|_| in_match)
            && self
                .waves
                .as_ref()
                .is_none_or(|old| newer(snap.seq, old.seq))
        {
            self.waves = Some(snap);
        }
        self.in_match = in_match;
        self.difficulty = difficulty.min(2);
        if (map as usize) < crate::MAPS.len() {
            self.map = map;
        }
        if starts {
            self.note_match_start();
        }
        // The countdown runs on here between rosters; a roster a little behind it
        // (sent a moment ago) doesn't wind it back, so each second ticks once.
        let follow = |here: Option<f32>, host: u16| {
            let host = (host != u16::MAX).then(|| host as f32 / 100.0);
            match (here, host) {
                (Some(here), Some(host)) if (here - host).abs() < COUNTDOWN_SLACK => {
                    Some(here.min(host))
                }
                (_, host) => host,
            }
        };
        self.countdown = follow(self.countdown, countdown);
        // (Likewise the countdown to the match proper.)
        self.match_countdown = follow(self.match_countdown, match_countdown);
    }

    fn read_state(&mut self, slot: usize, r: &mut Reader) {
        let clock = self.clock;
        let Some(remote) = self.remote.get_mut(slot) else {
            return;
        };
        let (Some(seq), Some(snapshot)) = (r.u32(), JazzSnapshot::read(r)) else {
            return;
        };
        if remote.snapshot.is_none() || seq > remote.snap_seq {
            remote.snap_seq = seq;
            remote.snapshot = Some(snapshot);
            remote.received_at = clock;
            remote.fresh = true;
        }
        let Some(count) = r.u8() else {
            return;
        };
        let mut events = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (Some(seq), Some(event)) = (r.u32(), JazzEvent::read(r)) else {
                break;
            };
            events.push((seq, event));
        }
        // (They come newest first; each is played out once, in order.)
        events.sort_by_key(|(seq, _)| *seq);
        for (seq, event) in events {
            if remote.last_event.is_none_or(|last| seq > last) {
                remote.last_event = Some(seq);
                remote.events.push(event);
            }
        }
    }

    /// An enemy packet from the host (see `enemy_packets`): its clock (this PC's
    /// idea of it set by the least delayed), its events (those sent again taken in
    /// once each, in order), and its enemies' states, each with the host's time.
    fn read_enemies(&mut self, r: &mut Reader) {
        let Some(at) = r.f32() else {
            return;
        };
        let offset = at - self.sync_clock();
        let e = &mut self.enemies;
        if e.host_offset.is_none_or(|o| offset > o) {
            e.host_offset = Some(offset);
        }
        let Some(count) = r.u8() else {
            return;
        };
        let mut resent = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (Some(seq), Some(age), Some(event)) = (r.u32(), r.u8(), EnemyEvent::read(r)) else {
                return;
            };
            resent.push((seq, at - age_of(age), event));
        }
        resent.sort_by_key(|(seq, ..)| *seq);
        for (seq, when, event) in resent {
            if e.last_event
                .is_none_or(|last| (seq.wrapping_sub(last) as i32) > 0)
            {
                e.last_event = Some(seq);
                e.events_in.push((when, event));
            }
        }
        let Some(count) = r.u8() else {
            return;
        };
        for _ in 0..count {
            let (Some(age), Some(event)) = (r.u8(), EnemyEvent::read(r)) else {
                return;
            };
            e.events_in.push((at - age_of(age), event));
        }
        let Some(count) = r.u8() else {
            return;
        };
        for _ in 0..count {
            let Some(state) = EnemyState::read(r) else {
                return;
            };
            e.states_in.push((at, state));
        }
        // (A client that doesn't take them in, out of the game, keeps no more than
        // a few seconds' worth.)
        if e.states_in.len() > 4096 {
            e.states_in.drain(..2048);
        }
        if e.events_in.len() > 4096 {
            e.events_in.drain(..2048);
        }
    }

    // --- timing out ----------------------------------------------------------

    fn time_out(&mut self) {
        let clock = self.clock;
        self.challenges.retain(|c| clock - c.at < CHALLENGE_SECS);
        self.refused.retain(|&(_, until)| clock < until);
        let timeout = if self.in_match {
            MATCH_TIMEOUT
        } else {
            TIMEOUT
        };
        let gone: Vec<usize> = match &self.role {
            Role::Host { peers } => peers
                .iter()
                .filter(|p| clock - p.last_heard > timeout)
                .map(|p| p.slot)
                .collect(),
            _ => Vec::new(),
        };
        for slot in gone {
            let who = self.player_name(slot);
            self.note(&format!(
                "gave {who} up: nothing had come from him for {timeout:.0} s"
            ));
            self.drop_peer(slot);
        }
        if let Role::Client {
            host,
            welcomed,
            started,
            last_heard,
            ..
        } = &self.role
        {
            let failure = if !*welcomed && clock - *started > CONNECT_TIMEOUT {
                Some((Failure::Connect, format!("no answer from a game at {host}")))
            } else if *welcomed && clock - *last_heard > timeout {
                Some((
                    Failure::HostLost,
                    format!("lost the host: nothing had come from it for {timeout:.0} s"),
                ))
            } else {
                None
            };
            if let Some((failure, why)) = failure {
                self.note(&why);
                self.failure = Some(failure);
                self.go_offline();
            }
        }
    }

    // --- sending -------------------------------------------------------------

    fn send(&mut self, dt: f32) {
        if self.socket.is_none() {
            return;
        }
        let clock = self.clock;
        match &mut self.role {
            Role::Offline => return,
            Role::Client {
                host,
                welcomed: false,
                last_hello,
                ..
            } => {
                if clock - *last_hello >= HELLO_EVERY {
                    *last_hello = clock;
                    let host = *host;
                    let mut w = Writer::new(HELLO);
                    w.u8(VERSION);
                    w.str8(&self.name);
                    self.send_to(host, &w.0);
                }
                return;
            }
            _ => {}
        }

        if self.is_host() {
            self.roster_timer -= dt;
            if self.roster_dirty || self.roster_timer <= 0.0 {
                self.roster_dirty = false;
                self.roster_timer = ROSTER_EVERY;
                let packet = self.roster_packet();
                self.send_peers(&packet);
            }
            self.ping_timer -= dt;
            if self.ping_timer <= 0.0 {
                self.ping_timer = PING_EVERY;
                let token = (clock * 1000.0) as u32;
                let mut w = Writer::new(PING);
                w.u32(token);
                if let Role::Host { peers } = &mut self.role {
                    for peer in peers.iter_mut() {
                        peer.ping = Some((token, clock));
                    }
                }
                self.send_peers(&w.0);
            }
        }

        // This PC's Jazz, in a match.
        self.state_timer -= dt;
        if self.in_match && self.state_timer <= 0.0 {
            self.state_timer = (self.state_timer + STATE_EVERY).max(0.0);
            if let Some(packet) = self.state_packet() {
                match &self.role {
                    Role::Host { .. } => self.send_peers(&packet),
                    Role::Client { host, .. } => self.send_to(*host, &packet),
                    Role::Offline => {}
                }
            }
        }

        // The host's enemies, in a match.
        if self.is_host() && self.in_match {
            self.enemies.timer -= dt;
            if self.enemies.timer <= 0.0 {
                self.enemies.timer = (self.enemies.timer + ENEMIES_EVERY).max(0.0);
                for packet in self.enemy_packets() {
                    self.send_peers(&packet);
                }
            }
        }
    }

    fn send_peers(&self, data: &[u8]) {
        if let Role::Host { peers } = &self.role {
            for peer in peers {
                self.send_to(peer.addr, data);
            }
        }
    }

    /// The lobby: whether the match has started, the countdown (hundredths of a
    /// second, or none), the difficulty, the map, the countdown to the match proper
    /// (as the countdown), then each player's slot, ping, score, name and whether
    /// he is a Prime.
    fn roster_packet(&mut self) -> Vec<u8> {
        // (Each player's ping, as the host last timed it.)
        if let Role::Host { peers } = &self.role {
            for peer in peers {
                if let Some(Some(p)) = self.players.get_mut(peer.slot) {
                    p.ping_ms = peer.ping_ms;
                }
            }
        }
        let mut w = Writer::new(ROSTER);
        w.u8(u8::from(self.in_match));
        w.u16(
            self.countdown
                .map(|c| (c * 100.0).clamp(0.0, 60000.0) as u16)
                .unwrap_or(u16::MAX),
        );
        w.u8(self.difficulty);
        w.u8(self.map);
        w.u16(
            self.match_countdown
                .map(|c| (c * 100.0).clamp(0.0, 60000.0) as u16)
                .unwrap_or(u16::MAX),
        );
        w.u8(self.player_count() as u8);
        for (slot, p) in self.players.iter().enumerate() {
            if let Some(p) = p {
                w.u8(slot as u8);
                w.u16(p.ping_ms);
                w.u16(p.score);
                w.str8(&p.name);
                w.u8(u8::from(p.prime));
            }
        }
        // The team's lives, failed rounds and game over (255 lives: none counted).
        let (lives, round, over) = self.team.unwrap_or((u8::MAX, 0, false));
        w.u8(lives);
        w.u8(round);
        w.u8(u8::from(over));
        // The waves, in a match.
        match self.waves.as_ref().filter(|_| self.in_match) {
            Some(snap) => {
                w.u8(1);
                snap.write(&mut w);
            }
            None => w.u8(0),
        }
        w.0
    }

    /// This PC's Jazz: his slot, the snapshot's number, the snapshot, then the
    /// events still going out (newest first, as many as fit).
    fn state_packet(&mut self) -> Option<Vec<u8>> {
        let mut snapshot = self.local?;
        let slot = self.me?;
        if self.chosen_here {
            snapshot.flags |= flag::CHOSEN;
        }
        snapshot.class = self.class_here.unwrap_or(u8::MAX);
        snapshot.stats = self.stats_here;
        self.snap_seq = self.snap_seq.wrapping_add(1);
        let mut w = Writer::new(STATE);
        w.u8(slot as u8);
        w.u32(self.snap_seq);
        snapshot.write(&mut w);
        let count_at = w.0.len();
        w.u8(0);
        let mut count = 0u8;
        let mut bytes = 0;
        for out in self.outbox.iter_mut().rev() {
            let mut e = Writer(Vec::new());
            e.u32(out.seq);
            out.event.write(&mut e);
            if bytes + e.0.len() > EVENT_BUDGET || count == u8::MAX {
                break;
            }
            bytes += e.0.len();
            w.0.extend_from_slice(&e.0);
            count += 1;
            out.sent = out.sent.saturating_add(1);
        }
        w.0[count_at] = count;
        let clock = self.clock;
        self.outbox.retain(|out| {
            let age = clock - out.at;
            age <= EVENT_WINDOW || (out.sent < 3 && age <= EVENT_MAX_AGE)
        });
        Some(w.0)
    }

    /// The host's enemies, as many packets as they need: each its clock (the time its
    /// states are from), then its enemies' events (the first packet's: those sent
    /// again for a while, newest first, each its number and how long ago it happened;
    /// then those sent once, oldest first, each how long ago), then the states of as
    /// many enemies as fit.
    fn enemy_packets(&mut self) -> Vec<Vec<u8>> {
        let clock = self.sync_clock();
        let e = &mut self.enemies;
        let mut packets = Vec::new();
        let mut w = Writer::new(ENEMIES);
        w.f32(clock);
        let count_at = w.0.len();
        w.u8(0);
        let (mut count, mut bytes) = (0u8, 0);
        for out in e.resent.iter_mut().rev() {
            let mut b = Writer(Vec::new());
            b.u32(out.seq);
            b.u8(age_byte(clock - out.at));
            out.event.write(&mut b);
            if bytes + b.0.len() > ENEMY_RESENT_BUDGET || count == u8::MAX {
                break;
            }
            bytes += b.0.len();
            w.0.extend_from_slice(&b.0);
            count += 1;
            out.sent = out.sent.saturating_add(1);
        }
        w.0[count_at] = count;
        e.resent.retain(|out| {
            let age = clock - out.at;
            age <= EVENT_WINDOW || (out.sent < 3 && age <= EVENT_MAX_AGE)
        });
        e.once.retain(|(at, _)| clock - at <= ENEMY_ONCE_MAX_AGE);
        let count_at = w.0.len();
        w.u8(0);
        let (mut count, mut bytes) = (0u8, 0);
        while let Some((at, event)) = e.once.front() {
            let mut b = Writer(Vec::new());
            b.u8(age_byte(clock - at));
            event.write(&mut b);
            if bytes + b.0.len() > ENEMY_ONCE_BUDGET || count == u8::MAX {
                break;
            }
            bytes += b.0.len();
            w.0.extend_from_slice(&b.0);
            count += 1;
            e.once.pop_front();
        }
        w.0[count_at] = count;
        let mut states = e.states.iter().peekable();
        loop {
            let count_at = w.0.len();
            w.u8(0);
            let mut count = 0u8;
            while let Some(s) = states.peek() {
                let mut b = Writer(Vec::new());
                s.write(&mut b);
                if (w.0.len() + b.0.len() > ENEMY_PACKET_MAX && count > 0) || count == u8::MAX {
                    break;
                }
                w.0.extend_from_slice(&b.0);
                count += 1;
                states.next();
            }
            w.0[count_at] = count;
            packets.push(std::mem::take(&mut w.0));
            if states.peek().is_none() {
                break;
            }
            // (The next: its clock, no events.)
            w = Writer::new(ENEMIES);
            w.f32(clock);
            w.u8(0);
            w.u8(0);
        }
        packets
    }
}

/// How long ago an event happened, as its packet carries it (4 ms steps, up to a
/// second), and back.
fn age_byte(secs: f32) -> u8 {
    (secs / 0.004).round().clamp(0.0, 255.0) as u8
}

fn age_of(byte: u8) -> f32 {
    byte as f32 * 0.004
}

/// Reads what came in (before the game's systems run).
pub fn net_receive(time: Res<Time<Real>>, mut net: ResMut<Net>) {
    // (A long frame, while online, to the net log.)
    net.note_frame(time.delta_secs());
    let dt = time.delta_secs().min(0.25);
    net.tick(dt);
}

impl Net {
    /// `dt` seconds on: what came in, who has gone, and the countdowns.
    fn tick(&mut self, dt: f32) {
        self.clock += dt;
        // (The host's clock as last set eases back, for the next packets to set.)
        if let Some(o) = self.enemies.host_offset.as_mut() {
            *o -= HOST_CLOCK_EASE * dt;
        }
        self.receive();
        self.time_out();
        // The countdown runs on between the host's rosters; the host starts the
        // match when it ends.
        if let Some(left) = self.countdown.as_mut() {
            *left = (*left - dt).max(0.0);
        }
        if self.is_host() && self.countdown == Some(0.0) && !self.in_match {
            self.countdown = None;
            self.in_match = true;
            self.roster_dirty = true;
            self.reset_match();
            self.note_match_start();
        }
        // In the match: once every player has chosen his character the host
        // starts the countdown to the match proper; it runs on here between the
        // host's rosters, as the lobby's does, and stays at 0 once it has run out.
        if self.in_match {
            if self.is_host() && self.match_countdown.is_none() && self.all_chosen() {
                self.match_countdown = Some(crate::pregame::COUNTDOWN_SECS as f32);
                self.roster_dirty = true;
            }
            if let Some(left) = self.match_countdown.as_mut() {
                *left = (*left - dt).max(0.0);
            }
        }
    }
}

/// Sends what is due (after the game's systems have run).
pub fn net_send(time: Res<Time<Real>>, mut net: ResMut<Net>) {
    let dt = time.delta_secs().min(0.25);
    net.send(dt);
}

// ---------------------------------------------------------------------------
// The net log
// ---------------------------------------------------------------------------

/// netlog.txt, by settings.txt: online play's comings and goings, each player
/// given up and why, the silences that came near it, and this PC's long stops
/// while online, each with the time (UTC) and the wave: for a game that broke up
/// to be looked into (both sides' logs). Begun afresh once past NET_LOG_MAX bytes.
/// (The tests write none.)
#[cfg_attr(test, allow(dead_code))]
const NET_LOG: &str = "netlog.txt";
#[cfg_attr(test, allow(dead_code))]
const NET_LOG_MAX: u64 = 256 * 1024;
/// A frame this long (s) while online is logged, and a silence this long from a
/// player (or the host) that then ends.
const LONG_FRAME: f32 = 1.0;
const LONG_SILENCE: f32 = 3.0;
/// The most long frames logged a game (a PC that keeps stopping fills no more).
const LONG_FRAMES_LOGGED: u32 = 100;

/// A line of the net log.
fn write_net_log(line: &str) {
    info!("netlog: {line}");
    #[cfg(not(test))]
    {
        use std::io::Write;
        let path = crate::assets_dir()
            .parent()
            .map(|p| p.join(NET_LOG))
            .unwrap_or_else(|| std::path::PathBuf::from(NET_LOG));
        let fresh = std::fs::metadata(&path).is_ok_and(|m| m.len() > NET_LOG_MAX);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(!fresh)
            .truncate(fresh)
            .open(&path);
        if let Ok(mut file) = file {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// The date and time now (UTC), as the logs give it (crash.txt's too).
pub fn utc_now() -> String {
    utc(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default())
}

/// The date and time (UTC) `secs` after 1970 began.
fn utc(secs: u64) -> String {
    // (Days to the civil date: Howard Hinnant's days_from_civil, backwards.)
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let t = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

impl Net {
    /// A line in the net log: when, the wave (in a match), what happened. (None
    /// for an Offline Game, which never goes online.)
    fn note(&self, what: &str) {
        if self.is_offline_game() {
            return;
        }
        let wave = match self.waves.as_ref().filter(|_| self.in_match) {
            Some(w) if w.wave >= 0 => format!(" (wave {})", i32::from(w.wave) + 1),
            Some(_) => " (before the first wave)".to_string(),
            None if self.in_match => " (in the match)".to_string(),
            None => String::new(),
        };
        write_net_log(&format!("{}{wave}: {what}", utc_now()));
    }

    /// The name of the player in `slot`, for the net log.
    fn player_name(&self, slot: usize) -> String {
        match self.players.get(slot) {
            Some(Some(p)) => format!("{} (player {})", p.name, slot + 1),
            _ => format!("player {}", slot + 1),
        }
    }

    /// The match has begun: its map and players.
    fn note_match_start(&self) {
        let map = crate::MAPS
            .get(self.map as usize)
            .map(|m| m.name)
            .unwrap_or("?");
        let players: Vec<String> = (0..SLOTS)
            .filter(|&s| self.players[s].is_some())
            .map(|s| self.player_name(s))
            .collect();
        self.note(&format!(
            "the match began on map {map}, with {}",
            players.join(", ")
        ));
    }

    /// A frame `secs` long: logged if long, while online.
    fn note_frame(&mut self, secs: f32) {
        if secs > LONG_FRAME
            && !matches!(self.role, Role::Offline)
            && self.socket.is_some()
            && self.long_frames < LONG_FRAMES_LOGGED
        {
            self.long_frames += 1;
            self.note(&format!(
                "this PC stopped for {secs:.1} s (a frame that long: the others heard nothing from it meanwhile)"
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// What is sent about a Jazz
// ---------------------------------------------------------------------------

/// Bits of `JazzSnapshot::flags`.
pub mod flag {
    pub const GROUNDED: u16 = 1;
    pub const VEHICLE: u16 = 2;
    pub const SPRINTING: u16 = 4;
    pub const AIMING: u16 = 8;
    pub const FIRING: u16 = 16;
    pub const LEFT_HANDED: u16 = 32;
    pub const DYING: u16 = 64;
    /// The car hovers (not boosting on its wheels).
    pub const HOVER: u16 = 128;
    /// The trigger is held (the car's guns fold out).
    pub const TRIGGER: u16 = 256;
    /// The Nucleon Charge Rifle charging, and fully charged (its scope's sounds
    /// and the glow round its barrel).
    pub const CHARGING: u16 = 512;
    pub const CHARGED: u16 = 1024;
    /// Not in the match yet: his player is choosing his character or counting
    /// down to it (pregame.rs). His Jazz isn't shown.
    pub const WAITING: u16 = 2048;
    /// His player has chosen his character (the host starts the countdown to the
    /// match once everyone's has).
    pub const CHOSEN: u16 = 4096;
    /// His gun warming up (the X18 Scrapmaker's barrels spinning up before its first
    /// round: its wind-up's sound).
    pub const WARMING: u16 = 8192;
    /// The blades loaded onto his Gear Shredder, 0 to 3, in two bits (their effects
    /// on his gun, and its charge's loop).
    pub const BLADES_LOW: u16 = 16384;
    pub const BLADES_HIGH: u16 = 32768;
}

/// A player's match so far, as the scoreboard shows it (scoreboard.rs): his level
/// in his character's class, his score (all the money his kills have paid, as
/// TnPlayerReplicationInfoSurvival.AddScore adds both), kills, deaths, revives and
/// the money he has now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchStats {
    pub level: u8,
    pub score: u32,
    pub kills: u16,
    pub deaths: u16,
    pub revives: u16,
    pub money: u32,
}

impl MatchStats {
    fn write(&self, w: &mut Writer) {
        w.u8(self.level);
        w.u32(self.score);
        w.u16(self.kills);
        w.u16(self.deaths);
        w.u16(self.revives);
        w.u32(self.money);
    }

    fn read(r: &mut Reader) -> Option<MatchStats> {
        Some(MatchStats {
            level: r.u8()?,
            score: r.u32()?,
            kills: r.u16()?,
            deaths: r.u16()?,
            revives: r.u16()?,
            money: r.u32()?,
        })
    }
}

/// A player as the scoreboard lists him ([`Net::scoreboard`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreLine {
    pub slot: usize,
    pub name: String,
    pub me: bool,
    pub class: Option<usize>,
    pub stats: MatchStats,
    pub ping_ms: u16,
}

/// A Jazz as his PC has him: what his copy on the other PCs needs to move and
/// animate as he does (main.rs's Player fields of the same names).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JazzSnapshot {
    pub pos: Vec3,
    pub heading: f32,
    /// Horizontal velocity.
    pub velocity: Vec3,
    pub vertical_velocity: f32,
    pub flags: u16,
    pub transform_left: f32,
    pub transform_clip: u8,
    pub move_input: Vec2,
    pub twist: f32,
    /// The view's pitch (radians, up positive): his tank's cannon follows it.
    pub aim_pitch: f32,
    /// The turn on the spot playing (its clip), or 255.
    pub pivot_clip: u8,
    /// The one-off move playing (its clip, or 255) and its seconds left.
    pub action_clip: u8,
    pub action_left: f32,
    /// Seconds since he jumped (negative: not jumping).
    pub jump_time: f32,
    pub air_time: f32,
    pub weapon: u8,
    pub hand: u8,
    pub loadout: [u8; 2],
    /// The upgrades on his weapons (upgrades.rs's Installed: bit i UPGRADES[i]).
    pub upgrades: u64,
    pub dead_left: f32,
    pub health: f32,
    pub shield: f32,
    pub hover_vel: Vec3,
    pub wheel_angle: f32,
    pub yaw_rate: f32,
    pub drive_speed: f32,
    pub strafe_speed: f32,
    /// His car's or truck's wheels' steering (Player::steer: they turn and the body
    /// leans by it).
    pub steer: f32,
    pub carry_vel: Vec3,
    /// His character's look (character.rs's Look::to_bytes): the parts he wears,
    /// their paint and glow, and so his vehicle.
    pub look: [u8; crate::character::Look::BYTES],
    /// The class of the character his player has chosen for the match (255: none
    /// yet): one of each class in a match.
    pub class: u8,
    /// His player's match so far (the scoreboard's).
    pub stats: MatchStats,
    /// Down, getting up, reviving another or dead till the round ends (revive.rs).
    pub revive: ReviveSnap,
    /// His class ability (abilities.rs).
    pub ability: AbilitySnap,
}

/// A Jazz's class ability (abilities.rs's ClassAbility): which (Kind's number, 0
/// none) and its upgrade, what is going (ACTIVE, his hover's phase in bits 1 and 2,
/// his shield broken, Battle Medic's buff on him), the seconds into it, whom his Heal
/// Beam heals (a slot, 255 none) and his Battle Sentry.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AbilitySnap {
    pub kind: u8,
    pub upgrade: u8,
    pub flags: u8,
    pub clock: f32,
    /// How full its meter is (0 to 255: empty to full).
    pub meter: u8,
    pub target: u8,
    pub sentry: SentrySnap,
}

impl AbilitySnap {
    pub const ACTIVE: u8 = 1;
    pub const SHIELD_BROKEN: u8 = 8;
    pub const MEDIC: u8 = 16;

    fn write(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.u8(self.upgrade);
        w.u8(self.flags);
        w.f32(self.clock);
        w.u8(self.meter);
        w.u8(self.target);
        self.sentry.write(w);
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(AbilitySnap {
            kind: r.u8()?,
            upgrade: r.u8()?,
            flags: r.u8()?,
            clock: r.f32()?,
            meter: r.u8()?,
            target: r.u8()?,
            sentry: SentrySnap::read(r)?,
        })
    }
}

/// A Jazz's Battle Sentry (battlesentry.rs's SentryLink): out, waking, on a target,
/// firing, Rust's (its flags); where it hovers, its aim (heading and pitch, radians)
/// and how far its beam reaches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SentrySnap {
    pub flags: u8,
    pub at: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub reach: f32,
}

impl SentrySnap {
    fn write(&self, w: &mut Writer) {
        w.u8(self.flags);
        if self.flags != 0 {
            w.vec3(self.at);
            w.f32(self.yaw);
            w.f32(self.pitch);
            w.f32(self.reach);
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        let flags = r.u8()?;
        if flags == 0 {
            return Some(SentrySnap::default());
        }
        Some(SentrySnap {
            flags,
            at: r.vec3()?,
            yaw: r.f32()?,
            pitch: r.f32()?,
            reach: r.f32()?,
        })
    }
}

/// A Jazz's downed state (revive.rs's Revive): which state he is in and the
/// seconds into it (his fall, his get-up, his revive of another, his death), his
/// downed health left, who revives him and whom he revives (slots, 255 none), a
/// turn over where he lies (1 left, -1 right, 0 none) and the seconds into it, and
/// whether he died down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReviveSnap {
    pub state: u8,
    pub clock: f32,
    pub downed_left: f32,
    pub reviver: u8,
    pub target: u8,
    pub pivot: i8,
    pub pivot_t: f32,
    pub died_downed: bool,
}

impl ReviveSnap {
    pub const UP: u8 = 0;
    pub const DOWNED: u8 = 1;
    pub const GETTING_UP: u8 = 2;
    pub const HEALING: u8 = 3;
    pub const OUT: u8 = 4;

    fn write(&self, w: &mut Writer) {
        w.u8(self.state);
        w.f32(self.clock);
        w.f32(self.downed_left);
        w.u8(self.reviver);
        w.u8(self.target);
        w.u8(self.pivot as u8);
        w.f32(self.pivot_t);
        w.u8(u8::from(self.died_downed));
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(ReviveSnap {
            state: r.u8()?,
            clock: r.f32()?,
            downed_left: r.f32()?,
            reviver: r.u8()?,
            target: r.u8()?,
            pivot: r.u8()? as i8,
            pivot_t: r.f32()?,
            died_downed: r.u8()? != 0,
        })
    }
}

impl JazzSnapshot {
    pub fn has(&self, bit: u16) -> bool {
        self.flags & bit != 0
    }

    fn write(&self, w: &mut Writer) {
        w.vec3(self.pos);
        w.f32(self.heading);
        w.vec3(self.velocity);
        w.f32(self.vertical_velocity);
        w.u16(self.flags);
        w.f32(self.transform_left);
        w.u8(self.transform_clip);
        w.f32(self.move_input.x);
        w.f32(self.move_input.y);
        w.f32(self.twist);
        w.f32(self.aim_pitch);
        w.u8(self.pivot_clip);
        w.u8(self.action_clip);
        w.f32(self.action_left);
        w.f32(self.jump_time);
        w.f32(self.air_time);
        w.u8(self.weapon);
        w.u8(self.hand);
        w.u8(self.loadout[0]);
        w.u8(self.loadout[1]);
        w.u32(self.upgrades as u32);
        w.u32((self.upgrades >> 32) as u32);
        w.f32(self.dead_left);
        w.f32(self.health);
        w.f32(self.shield);
        w.vec3(self.hover_vel);
        w.f32(self.wheel_angle);
        w.f32(self.yaw_rate);
        w.f32(self.drive_speed);
        w.f32(self.strafe_speed);
        w.f32(self.steer);
        w.vec3(self.carry_vel);
        for b in self.look {
            w.u8(b);
        }
        w.u8(self.class);
        self.stats.write(w);
        self.revive.write(w);
        self.ability.write(w);
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(JazzSnapshot {
            pos: r.vec3()?,
            heading: r.f32()?,
            velocity: r.vec3()?,
            vertical_velocity: r.f32()?,
            flags: r.u16()?,
            transform_left: r.f32()?,
            transform_clip: r.u8()?,
            move_input: Vec2::new(r.f32()?, r.f32()?),
            twist: r.f32()?,
            aim_pitch: r.f32()?,
            pivot_clip: r.u8()?,
            action_clip: r.u8()?,
            action_left: r.f32()?,
            jump_time: r.f32()?,
            air_time: r.f32()?,
            weapon: r.u8()?,
            hand: r.u8()?,
            loadout: [r.u8()?, r.u8()?],
            upgrades: r.u32()? as u64 | (r.u32()? as u64) << 32,
            dead_left: r.f32()?,
            health: r.f32()?,
            shield: r.f32()?,
            hover_vel: r.vec3()?,
            wheel_angle: r.f32()?,
            yaw_rate: r.f32()?,
            drive_speed: r.f32()?,
            strafe_speed: r.f32()?,
            steer: r.f32()?,
            carry_vel: r.vec3()?,
            look: {
                let mut look = [0u8; crate::character::Look::BYTES];
                for b in &mut look {
                    *b = r.u8()?;
                }
                look
            },
            class: r.u8()?,
            stats: MatchStats::read(r)?,
            revive: ReviveSnap::read(r)?,
            ability: AbilitySnap::read(r)?,
        })
    }
}

/// Whether `seq` comes after `last` (numbers that wrap round).
fn newer(seq: u16, last: u16) -> bool {
    (seq.wrapping_sub(last) as i16) > 0
}

/// Escalation's waves as the host has them (waves.rs), in its rosters: every PC
/// shows and plays them out from this. Its number (`seq`) goes up with each change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WaveSnap {
    pub seq: u16,
    /// The wave (TnSurvivalDataStoreWaveSpawning's _CurrentWave: from 0, -1 before
    /// the first).
    pub wave: i8,
    /// The wave's players' buffs are on (from its start till its end).
    pub on: bool,
    /// The countdown to the next wave (the GRI's IsCountingDown, CurrentCountdown).
    pub counting: bool,
    pub countdown: i16,
    /// The last enemies show on every player's HUD (TnBuffSeeEnemyObjectiveMarkers).
    pub markers: bool,
    /// The last few messages (TnSurvivalTimeMessage): each its number (counting
    /// up, wrapping) and its switch (its type plus 100 times its wave), oldest first.
    pub messages: Vec<(u8, u16)>,
    /// The waves' rewards paid so far, and the last one (each player's money).
    pub paid: (u8, u16),
    /// Rounds won (each brings the dead back and the downed up), and waves ended
    /// by force (their enemies gone at once, not killed).
    pub cleared: u8,
    pub purged: u8,
    /// The last wave is won.
    pub won: bool,
    /// The enemies the waves have in play.
    pub enemies: Vec<EnemySnap>,
}

/// The most enemies a roster carries (the waves keep 14 at most in play, and their
/// Titans' drones).
const WAVE_ENEMIES_MAX: usize = 32;
/// The most messages a roster carries.
const WAVE_MESSAGES_MAX: usize = 4;

/// An enemy the waves have brought in (or a Titan's drone: its Titan's number,
/// `owner`, 0 for the others): its number, its type (enemies.rs's Kinds), where it
/// came in and the way it faced, its look's seed, where it goes from there (where it
/// came in, but for one that flies or leaps in), and whether it leaps there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnemySnap {
    pub id: u32,
    pub kind: u8,
    pub at: Vec3,
    pub yaw: f32,
    pub seed: u32,
    pub to: Vec3,
    pub leap: bool,
    pub owner: u32,
}

impl WaveSnap {
    /// The same waves (all but the number).
    fn same(&self, other: &WaveSnap) -> bool {
        WaveSnap {
            seq: other.seq,
            ..self.clone()
        } == *other
    }

    fn write(&self, w: &mut Writer) {
        w.u16(self.seq);
        w.u8(self.wave as u8);
        let flags = [self.on, self.counting, self.markers, self.won]
            .iter()
            .enumerate()
            .fold(0u8, |bits, (i, on)| bits | (u8::from(*on) << i));
        w.u8(flags);
        w.u16(self.countdown as u16);
        let messages = &self.messages[self.messages.len().saturating_sub(WAVE_MESSAGES_MAX)..];
        w.u8(messages.len() as u8);
        for (seq, switch) in messages {
            w.u8(*seq);
            w.u16(*switch);
        }
        w.u8(self.paid.0);
        w.u16(self.paid.1);
        w.u8(self.cleared);
        w.u8(self.purged);
        let enemies = &self.enemies[..self.enemies.len().min(WAVE_ENEMIES_MAX)];
        w.u8(enemies.len() as u8);
        for e in enemies {
            w.u32(e.id);
            w.u8(e.kind);
            w.vec3(e.at);
            w.f32(e.yaw);
            w.u32(e.seed);
            w.vec3(e.to);
            w.u8(u8::from(e.leap));
            w.u32(e.owner);
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        let seq = r.u16()?;
        let wave = r.u8()? as i8;
        let flags = r.u8()?;
        let countdown = r.u16()? as i16;
        let mut messages = Vec::new();
        for _ in 0..r.u8()?.min(WAVE_MESSAGES_MAX as u8) {
            messages.push((r.u8()?, r.u16()?));
        }
        let paid = (r.u8()?, r.u16()?);
        let (cleared, purged) = (r.u8()?, r.u8()?);
        let mut enemies = Vec::new();
        for _ in 0..r.u8()?.min(WAVE_ENEMIES_MAX as u8) {
            enemies.push(EnemySnap {
                id: r.u32()?,
                kind: r.u8()?,
                at: r.vec3()?,
                yaw: r.f32()?,
                seed: r.u32()?,
                to: r.vec3()?,
                leap: r.u8()? != 0,
                owner: r.u32()?,
            });
        }
        Some(WaveSnap {
            seq,
            wave,
            on: flags & 1 != 0,
            counting: flags & 2 != 0,
            countdown,
            markers: flags & 4 != 0,
            messages,
            paid,
            cleared,
            purged,
            won: flags & 8 != 0,
            enemies,
        })
    }
}

/// Something a Jazz did, for his copies to do too.
#[derive(Clone, Debug, PartialEq)]
pub enum JazzEvent {
    /// A move's animation started (Player::start_clip).
    Clip { clip: u8, speed: f32 },
    /// The gun's one-off animation started (Player::start_weapon_clip).
    WeaponClip { clip: u8, secs: f32, speed: f32 },
    /// A weapon switch started (Player::start_equip; `old` 255 for none).
    Equip { new: u8, old: u8 },
    /// The gun started moving to the other hand.
    HandSwitch,
    /// The car jumped.
    CarJump,
    /// One of his sounds (a SoundBank cue), as his own PC heard it.
    Sound { cue: String, delay: f32, scale: f32 },
    /// A shot: the weapon (255 the car's guns), where it left and the way it went,
    /// how far it went, and what it struck (0 nothing, 1 a wall, 2 a sentry; with
    /// 0x80 added, a shotgun's further pellet of the shot before, main.rs's
    /// SHOT_PELLET; and the Nucleon Charge Rifle's charge level in bits 4 and 5,
    /// SHOT_LEVEL_MASK); the enemy it struck, by its number (the waves', 0 none or
    /// one not numbered: the one nearest where it struck).
    Shot {
        weapon: u8,
        origin: Vec3,
        dir: Vec3,
        distance: f32,
        struck: u8,
        enemy: u16,
    },
    /// A charging punch's blow landed: from his chest, facing `forward`.
    Melee { at: Vec3, forward: Vec3 },
    /// His sword swing's blow began (melee.rs): its box's centre, and his heading
    /// (radians), which turns it.
    Sweep { centre: Vec3, heading: f32 },
    /// One of the waves' enemies died at his PC (waves.rs): its number, how it died
    /// (enemies.rs's Death number), whether the blow gibs it (`flags` bit 0),
    /// where it struck and its momentum (Unreal's units, the demo's axes: its way and
    /// its size). It dies here too, the same death.
    EnemyDeath {
        id: u32,
        death: u8,
        flags: u8,
        at: Vec3,
        momentum: Vec3,
    },
    /// An EMP grenade thrown (emp.rs): where it left his hand, its velocity, and
    /// its fuse's seconds once it lands.
    EmpGrenade {
        start: Vec3,
        velocity: Vec3,
        fuse: f32,
    },
    /// A round fired from a weapon whose rounds fly (the Riot Cannon's, riot.rs):
    /// the weapon, where it left the muzzle and the way it went.
    Round { weapon: u8, origin: Vec3, dir: Vec3 },
    /// A homing round fired locked on (a jet's rocket, the Thermo Rocket Cannon's):
    /// as Round, and where the sentry it homes on was (its copies home on the one
    /// nearest there).
    HomingRound {
        weapon: u8,
        origin: Vec3,
        dir: Vec3,
        target: Vec3,
    },
    /// A round lobbed (the Chaos-Rift Combustor's): the weapon, where it left the
    /// muzzle and its velocity.
    Lob {
        weapon: u8,
        origin: Vec3,
        velocity: Vec3,
    },
    /// His weapon's rounds set off where they are (his A-4 Pulsar Cannon's alt fire,
    /// his Chaos-Rift Combustor's trigger bursting its bomb).
    SetOff { weapon: u8 },
    /// He died (downed.rs's DeathNews): how (enemies.rs's Death number), whether
    /// it struck his head (bit 0), whether he was in the air (bit 1) and whether it
    /// gibs him (bit 2), the number his death's choices are made from, where the blow
    /// struck and its momentum (`dir`: Unreal's units, the demo's axes, its way and
    /// its size).
    Death {
        death: u8,
        flags: u8,
        seed: u32,
        at: Vec3,
        dir: Vec3,
    },
    /// He used a support item (equipment.rs): which (shop::Equipment::index), where
    /// and the way it went: his Diffraction Barrier where it stands and the way it
    /// faces, his Dimensional Decimator's ball where it came out and the way it
    /// rolls, the others where he was and the way he looked.
    Gear { item: u8, at: Vec3, facing: Vec3 },
    /// One of his Thermo Mines let out, where it starts.
    Mine { at: Vec3 },
    /// A support item of his ended early: his barrier let down, his ball set off.
    GearEnd { item: u8 },
    /// His revive of a downed teammate (the teammate's slot): begun, stopped or
    /// done (revive.rs's STAGE_START, STAGE_ABORT, STAGE_DONE). The teammate's PC
    /// acts on it.
    Revive { target: u8, stage: u8 },
    /// He used the map's Armory Recreator `pod` (its number in the map's list,
    /// recreator.rs), which made `item` (recreator.rs's LOOT number): it opens
    /// here too, with the same thing in it.
    Pod { pod: u8, item: u8 },
    /// He took what the Armory Recreator `pod` made: it is gone here too.
    PodLoot { pod: u8 },
    /// He put energon into the map's unlock console `console` (its number in the
    /// map's list, doors.rs): its total grows here too.
    Deposit { console: u8 },
    /// His deposit into `console` filled its total: what it unlocks opens here too.
    Unlock { console: u8 },
    /// A death noticed at his PC, for the kill feed here (killfeed.rs's KillNote):
    /// its killer and its victim (each a kind, 0 nobody, 1 a player, 2 an enemy, 3 a
    /// trap's sentry, and its number: the player's slot, the enemy's type), its
    /// damage type (killfeed.rs's DAMAGE_TYPES), the enemy that died by its
    /// number in the waves (0 for none: a player's death) and whether it was a
    /// headshot.
    KillFeed {
        killer: u8,
        killer_id: u16,
        victim: u8,
        victim_id: u16,
        damage: u8,
        foe: u32,
        head: bool,
    },
    /// A moment of his class ability (abilities.rs) for its copies here: `what` it was
    /// (a Whirlwind's sweep, a Heal Beam's reward, a Flak Shield breaking, a Hover
    /// Slam landing, a Battle Sentry's round: their modules' WHAT_*), where, which
    /// way, and a number of its own.
    Ability {
        what: u8,
        at: Vec3,
        dir: Vec3,
        value: f32,
    },
    /// The map's exploding barrel `barrel` (its number in its list, barrels.rs) blew
    /// up at his PC, by his hand, or (`ai`) by his PC's AI's: it blows up here too,
    /// as theirs.
    BarrelBlown { barrel: u8, ai: bool },
}

impl JazzEvent {
    fn write(&self, w: &mut Writer) {
        match self {
            JazzEvent::Clip { clip, speed } => {
                w.u8(0);
                w.u8(*clip);
                w.f32(*speed);
            }
            JazzEvent::WeaponClip { clip, secs, speed } => {
                w.u8(1);
                w.u8(*clip);
                w.f32(*secs);
                w.f32(*speed);
            }
            JazzEvent::Equip { new, old } => {
                w.u8(2);
                w.u8(*new);
                w.u8(*old);
            }
            JazzEvent::HandSwitch => w.u8(3),
            JazzEvent::CarJump => w.u8(4),
            JazzEvent::Sound { cue, delay, scale } => {
                w.u8(5);
                w.str8(cue);
                w.f32(*delay);
                w.f32(*scale);
            }
            JazzEvent::Shot {
                weapon,
                origin,
                dir,
                distance,
                struck,
                enemy,
            } => {
                w.u8(6);
                w.u8(*weapon);
                w.vec3(*origin);
                w.vec3(*dir);
                w.f32(*distance);
                w.u8(*struck);
                w.u16(*enemy);
            }
            JazzEvent::Melee { at, forward } => {
                w.u8(8);
                w.vec3(*at);
                w.vec3(*forward);
            }
            JazzEvent::EmpGrenade {
                start,
                velocity,
                fuse,
            } => {
                w.u8(9);
                w.vec3(*start);
                w.vec3(*velocity);
                w.f32(*fuse);
            }
            JazzEvent::Round {
                weapon,
                origin,
                dir,
            } => {
                w.u8(10);
                w.u8(*weapon);
                w.vec3(*origin);
                w.vec3(*dir);
            }
            JazzEvent::HomingRound {
                weapon,
                origin,
                dir,
                target,
            } => {
                w.u8(11);
                w.u8(*weapon);
                w.vec3(*origin);
                w.vec3(*dir);
                w.vec3(*target);
            }
            JazzEvent::Lob {
                weapon,
                origin,
                velocity,
            } => {
                w.u8(12);
                w.u8(*weapon);
                w.vec3(*origin);
                w.vec3(*velocity);
            }
            JazzEvent::SetOff { weapon } => {
                w.u8(13);
                w.u8(*weapon);
            }
            JazzEvent::Death {
                death,
                flags,
                seed,
                at,
                dir,
            } => {
                w.u8(14);
                w.u8(*death);
                w.u8(*flags);
                w.u32(*seed);
                w.vec3(*at);
                w.vec3(*dir);
            }
            JazzEvent::Gear { item, at, facing } => {
                w.u8(15);
                w.u8(*item);
                w.vec3(*at);
                w.vec3(*facing);
            }
            JazzEvent::Mine { at } => {
                w.u8(16);
                w.vec3(*at);
            }
            JazzEvent::GearEnd { item } => {
                w.u8(17);
                w.u8(*item);
            }
            JazzEvent::Revive { target, stage } => {
                w.u8(18);
                w.u8(*target);
                w.u8(*stage);
            }
            JazzEvent::Pod { pod, item } => {
                w.u8(19);
                w.u8(*pod);
                w.u8(*item);
            }
            JazzEvent::PodLoot { pod } => {
                w.u8(20);
                w.u8(*pod);
            }
            JazzEvent::Deposit { console } => {
                w.u8(21);
                w.u8(*console);
            }
            JazzEvent::Unlock { console } => {
                w.u8(22);
                w.u8(*console);
            }
            JazzEvent::Ability {
                what,
                at,
                dir,
                value,
            } => {
                w.u8(23);
                w.u8(*what);
                w.vec3(*at);
                w.vec3(*dir);
                w.f32(*value);
            }
            JazzEvent::Sweep { centre, heading } => {
                w.u8(24);
                w.vec3(*centre);
                w.f32(*heading);
            }
            JazzEvent::EnemyDeath {
                id,
                death,
                flags,
                at,
                momentum,
            } => {
                w.u8(25);
                w.u32(*id);
                w.u8(*death);
                w.u8(*flags);
                w.vec3(*at);
                w.vec3(*momentum);
            }
            JazzEvent::KillFeed {
                killer,
                killer_id,
                victim,
                victim_id,
                damage,
                foe,
                head,
            } => {
                w.u8(26);
                w.u8(*killer);
                w.u16(*killer_id);
                w.u8(*victim);
                w.u16(*victim_id);
                w.u8(*damage);
                w.u32(*foe);
                w.u8(u8::from(*head));
            }
            JazzEvent::BarrelBlown { barrel, ai } => {
                w.u8(27);
                w.u8(*barrel);
                w.u8(*ai as u8);
            }
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(match r.u8()? {
            0 => JazzEvent::Clip {
                clip: r.u8()?,
                speed: r.f32()?,
            },
            1 => JazzEvent::WeaponClip {
                clip: r.u8()?,
                secs: r.f32()?,
                speed: r.f32()?,
            },
            2 => JazzEvent::Equip {
                new: r.u8()?,
                old: r.u8()?,
            },
            3 => JazzEvent::HandSwitch,
            4 => JazzEvent::CarJump,
            5 => JazzEvent::Sound {
                cue: r.str8()?,
                delay: r.f32()?,
                scale: r.f32()?,
            },
            6 => JazzEvent::Shot {
                weapon: r.u8()?,
                origin: r.vec3()?,
                dir: r.vec3()?,
                distance: r.f32()?,
                struck: r.u8()?,
                enemy: r.u16()?,
            },
            8 => JazzEvent::Melee {
                at: r.vec3()?,
                forward: r.vec3()?,
            },
            9 => JazzEvent::EmpGrenade {
                start: r.vec3()?,
                velocity: r.vec3()?,
                fuse: r.f32()?,
            },
            10 => JazzEvent::Round {
                weapon: r.u8()?,
                origin: r.vec3()?,
                dir: r.vec3()?,
            },
            11 => JazzEvent::HomingRound {
                weapon: r.u8()?,
                origin: r.vec3()?,
                dir: r.vec3()?,
                target: r.vec3()?,
            },
            12 => JazzEvent::Lob {
                weapon: r.u8()?,
                origin: r.vec3()?,
                velocity: r.vec3()?,
            },
            13 => JazzEvent::SetOff { weapon: r.u8()? },
            14 => JazzEvent::Death {
                death: r.u8()?,
                flags: r.u8()?,
                seed: r.u32()?,
                at: r.vec3()?,
                dir: r.vec3()?,
            },
            15 => JazzEvent::Gear {
                item: r.u8()?,
                at: r.vec3()?,
                facing: r.vec3()?,
            },
            16 => JazzEvent::Mine { at: r.vec3()? },
            17 => JazzEvent::GearEnd { item: r.u8()? },
            18 => JazzEvent::Revive {
                target: r.u8()?,
                stage: r.u8()?,
            },
            19 => JazzEvent::Pod {
                pod: r.u8()?,
                item: r.u8()?,
            },
            20 => JazzEvent::PodLoot { pod: r.u8()? },
            21 => JazzEvent::Deposit { console: r.u8()? },
            22 => JazzEvent::Unlock { console: r.u8()? },
            23 => JazzEvent::Ability {
                what: r.u8()?,
                at: r.vec3()?,
                dir: r.vec3()?,
                value: r.f32()?,
            },
            24 => JazzEvent::Sweep {
                centre: r.vec3()?,
                heading: r.f32()?,
            },
            25 => JazzEvent::EnemyDeath {
                id: r.u32()?,
                death: r.u8()?,
                flags: r.u8()?,
                at: r.vec3()?,
                momentum: r.vec3()?,
            },
            26 => JazzEvent::KillFeed {
                killer: r.u8()?,
                killer_id: r.u16()?,
                victim: r.u8()?,
                victim_id: r.u16()?,
                damage: r.u8()?,
                foe: r.u32()?,
                head: r.u8()? != 0,
            },
            27 => JazzEvent::BarrelBlown {
                barrel: r.u8()?,
                ai: r.u8()? != 0,
            },
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// What is sent about the host's enemies
// ---------------------------------------------------------------------------

/// Bits of `EnemyState::flags`.
pub mod enemy_flag {
    /// Its gun in hand (Gun::equipped), and a vehicle gun's meshes out (Gun::out).
    pub const EQUIPPED: u16 = 1;
    pub const OUT: u16 = 2;
    /// Its gun firing (its effects', Gun::effects_firing) and charging (the
    /// Guardian Blaster's): their loops.
    pub const FIRING: u16 = 4;
    pub const CHARGING: u16 = 8;
    /// The Guard's scanner beam on its head.
    pub const SCANNER: u16 = 16;
    /// Its plane form flying (boosting), not hovering.
    pub const BOOSTING: u16 = 32;
    /// Its laser sight on (the Sharpshooter's).
    pub const LASER: u16 = 64;
}

/// One of the host's enemies as its AI has it this frame (src/ai/sync.rs): its
/// number (the waves', EnemySnap's), its pawn's place and velocity (Unreal's space
/// and units), its rotation (yaw, pitch and roll: Unreal's rotation units), its
/// physics (UE3's EPhysics), what it is doing (`flags`: enemy_flag's), its gun (its
/// blueprint's, by its place there: 255 none), what it makes of blows (its damage
/// multiplier), its pose (its anim tree's), and for those that have them its forms,
/// its plane form's rotation (pitch, yaw and roll) and its laser sight's aim.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnemyState {
    pub id: u16,
    pub loc: Vec3,
    pub vel: Vec3,
    pub rot: [u16; 3],
    pub physics: u8,
    pub flags: u16,
    pub gun: u8,
    pub scale: f32,
    pub pose: Pose,
    pub forms: Option<FormsState>,
    pub plane: Option<[u16; 3]>,
    pub laser: Option<LaserState>,
}

/// A transforming enemy's forms (ai/forms.rs's Forms): the form it is in and the
/// one it is taking, each form's model hidden, its alt form's effects allowed, and a
/// change of form under way (the form it changes to, its seconds left and its
/// length) with the pose the form it leaves plays meanwhile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormsState {
    pub current: u8,
    pub target: u8,
    pub hidden: [bool; 2],
    pub fx_allowed: bool,
    pub change: Option<(u8, f32, f32)>,
    pub other: Option<Pose>,
}

/// A laser sight's aim (ai/pawn.rs's LaserSight): where it points and the point
/// easing towards it (Unreal's space), its beams' spread and their roll (degrees).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaserState {
    pub target: Vec3,
    pub smoothed: Vec3,
    pub spread: f32,
    pub roll: f32,
}

/// The host's enemies' pose's layer by its number.
fn layer_of(i: u8) -> Option<Layer> {
    Some(match i {
        0 => Layer::Base,
        1 => Layer::BaseAdd,
        2 => Layer::Aim,
        3 => Layer::Upper,
        4 => Layer::LeftArm,
        5 => Layer::Full,
        6 => Layer::Jump,
        7 => Layer::Add,
        _ => return None,
    })
}

/// A pose (ai/tree.rs's): its clips (each its layer and clip in two bytes, its
/// weight in 16384ths and its time in milliseconds), its override layers' shares
/// (those it has, after a byte saying which, in 65535ths), its mirror (255ths) and its
/// aim offset (its profile, 255 none, and its aim in 16384ths).
fn write_pose(w: &mut Writer, p: &Pose) {
    let clips = &p.clips[..p.clips.len().min(u8::MAX as usize)];
    w.u8(clips.len() as u8);
    for c in clips {
        w.u16(((c.layer as u16) << 13) | (c.clip & 0x1fff));
        w.u16((c.weight * 16384.0).round().clamp(0.0, 65535.0) as u16);
        w.u16((c.time * 1000.0).round().clamp(0.0, 65535.0) as u16);
    }
    let mut mask = 0u8;
    for (i, s) in p.shares.iter().enumerate() {
        if *s > 0.0 {
            mask |= 1 << i;
        }
    }
    w.u8(mask);
    for s in p.shares.iter().filter(|s| **s > 0.0) {
        w.u16((s.clamp(0.0, 1.0) * 65535.0).round() as u16);
    }
    w.u8((p.mirror.clamp(0.0, 1.0) * 255.0).round() as u8);
    match p.aim_offset {
        Some((profile, aim)) => {
            w.u8(profile.min(254));
            w.i16((aim.x * 16384.0).round().clamp(-32767.0, 32767.0) as i16);
            w.i16((aim.y * 16384.0).round().clamp(-32767.0, 32767.0) as i16);
        }
        None => w.u8(255),
    }
}

fn read_pose(r: &mut Reader) -> Option<Pose> {
    let n = r.u8()?;
    let mut clips = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let (id, weight, time) = (r.u16()?, r.u16()?, r.u16()?);
        clips.push(PoseClip {
            layer: layer_of((id >> 13) as u8)?,
            clip: id & 0x1fff,
            weight: weight as f32 / 16384.0,
            time: time as f32 / 1000.0,
        });
    }
    let mask = r.u8()?;
    let mut shares = [0.0; LAYERS];
    for (i, s) in shares.iter_mut().enumerate() {
        if mask & (1 << i) != 0 {
            *s = r.u16()? as f32 / 65535.0;
        }
    }
    let mirror = r.u8()? as f32 / 255.0;
    let aim_offset = match r.u8()? {
        255 => None,
        profile => Some((
            profile,
            Vec2::new(r.i16()? as f32 / 16384.0, r.i16()? as f32 / 16384.0),
        )),
    };
    Some(Pose {
        clips,
        shares,
        mirror,
        aim_offset,
    })
}

impl EnemyState {
    fn write(&self, w: &mut Writer) {
        w.u16(self.id);
        w.vec3(self.loc);
        // (Its velocity to the unit a second: up to 327 m/s.)
        for v in self.vel.to_array() {
            w.i16(v.round().clamp(-32767.0, 32767.0) as i16);
        }
        for r in self.rot {
            w.u16(r);
        }
        w.u8(self.physics);
        // (Which of the parts not every enemy has follow.)
        w.u8(u8::from(self.forms.is_some())
            | u8::from(self.plane.is_some()) << 1
            | u8::from(self.laser.is_some()) << 2);
        w.u16(self.flags);
        w.u8(self.gun);
        w.u16((self.scale * 1000.0).round().clamp(0.0, 65535.0) as u16);
        write_pose(w, &self.pose);
        if let Some(f) = &self.forms {
            w.u8(f.current);
            w.u8(f.target);
            w.u8(u8::from(f.hidden[0])
                | u8::from(f.hidden[1]) << 1
                | u8::from(f.fx_allowed) << 2
                | u8::from(f.change.is_some()) << 3
                | u8::from(f.other.is_some()) << 4);
            if let Some((to, left, duration)) = f.change {
                w.u8(to);
                w.f32(left);
                w.f32(duration);
            }
            if let Some(other) = &f.other {
                write_pose(w, other);
            }
        }
        if let Some(rot) = self.plane {
            for r in rot {
                w.u16(r);
            }
        }
        if let Some(l) = &self.laser {
            w.vec3(l.target);
            w.vec3(l.smoothed);
            w.f32(l.spread);
            w.f32(l.roll);
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        let id = r.u16()?;
        let loc = r.vec3()?;
        let vel = Vec3::new(r.i16()? as f32, r.i16()? as f32, r.i16()? as f32);
        let rot = [r.u16()?, r.u16()?, r.u16()?];
        let physics = r.u8()?;
        let parts = r.u8()?;
        let flags = r.u16()?;
        let gun = r.u8()?;
        let scale = r.u16()? as f32 / 1000.0;
        let pose = read_pose(r)?;
        let forms = if parts & 1 != 0 {
            let current = r.u8()?;
            let target = r.u8()?;
            let bits = r.u8()?;
            let change = if bits & 8 != 0 {
                Some((r.u8()?, r.f32()?, r.f32()?))
            } else {
                None
            };
            let other = if bits & 16 != 0 {
                Some(read_pose(r)?)
            } else {
                None
            };
            Some(FormsState {
                current,
                target,
                hidden: [bits & 1 != 0, bits & 2 != 0],
                fx_allowed: bits & 4 != 0,
                change,
                other,
            })
        } else {
            None
        };
        let plane = if parts & 2 != 0 {
            Some([r.u16()?, r.u16()?, r.u16()?])
        } else {
            None
        };
        let laser = if parts & 4 != 0 {
            Some(LaserState {
                target: r.vec3()?,
                smoothed: r.vec3()?,
                spread: r.f32()?,
                roll: r.f32()?,
            })
        } else {
            None
        };
        Some(EnemyState {
            id,
            loc,
            vel,
            rot,
            physics,
            flags,
            gun,
            scale,
            pose,
            forms,
            plane,
            laser,
        })
    }
}

/// Bits of `EnemyEvent::Shot`'s flags: it shows its tracer, is its round's first
/// trace (its muzzle flash), and is a trace at all (none for a projectile's round:
/// its flash only).
pub mod shot_flag {
    pub const TRACER: u8 = 1;
    pub const FIRST: u8 = 2;
    pub const TRACED: u8 = 4;
}

/// Bits of `EnemyEvent::Hurt`'s flags: a melee blow, a blow that jams him (the
/// Guard's area attack), a gun round's that gibs him should it kill him.
pub mod hurt_flag {
    pub const MELEE: u8 = 1;
    pub const JAM: u8 = 2;
    pub const GIB: u8 = 4;
}

/// Bits of `EnemyEvent::Effect`'s flags: it stays on its socket (carried with
/// it), plays only if its last isn't still playing, starts its last over, or is
/// turned off with its animation's clip.
pub mod effect_flag {
    pub const ATTACHED: u8 = 1;
    pub const ONLY_ONCE: u8 = 2;
    pub const RESTART: u8 = 4;
    /// Turned off with its clip, its `off` seconds after it starts.
    pub const AUTO_OFF: u8 = 8;
}

/// Something the host's enemies did (its AI's outputs, ai/mod.rs's AiOutputs), for
/// their copies on the clients to do too: each enemy by its number, each player by
/// his slot, places in Unreal's space.
#[derive(Clone, Debug, PartialEq)]
pub enum EnemyEvent {
    /// A round's trace (shared::Shot; a projectile's flash, without TRACED): from
    /// where to where, its gun's effects (ai/mod.rs's GUN_FX, by their place in the
    /// list: 255 the default), its shot_flag bits and the normal of the wall it
    /// struck.
    Shot {
        by: u16,
        from: Vec3,
        to: Vec3,
        gun: u8,
        flags: u8,
        normal: Option<Vec3>,
    },
    /// A projectile fired (shared::Launch): where from, its way, its class (its
    /// ProjectileData's) and the player it homes on (his slot, 255 none).
    Launch {
        by: u16,
        from: Vec3,
        dir: Vec3,
        class: String,
        target: u8,
    },
    /// A grenade thrown (shared::Throw): its class, where from and its velocity.
    Throw {
        by: u16,
        class: String,
        from: Vec3,
        velocity: Vec3,
    },
    /// A blow to a player (shared::PlayerDamage): his slot, its damage (before his
    /// buffs: his PC's), where it struck and its way, its hurt_flag bits, its push,
    /// and a gun round's death (enemies.rs's Death) and momentum, and its damage
    /// type (the kill feed's: its index in killfeed.rs's DAMAGE_TYPES).
    Hurt {
        slot: u8,
        by: u16,
        amount: f32,
        at: Vec3,
        dir: Vec3,
        flags: u8,
        momentum: Option<Vec3>,
        shot: Option<(u8, f32)>,
        damage: u8,
    },
    /// A blow to a Diffraction Barrier (equipment.rs's): where it stands (the
    /// demo's space), its damage.
    Barrier { at: Vec3, amount: f32 },
    /// A player a Guard's scanner caught (guard::Scanned): his slot.
    Scanned { slot: u8, by: u16 },
    /// A sound at a place (its cue).
    Sound { by: u16, cue: String, at: Vec3 },
    /// A line of dialog said (audio::Say::Line): its owner, the line it cuts short,
    /// its cue and where.
    Line {
        owner: u64,
        cut: Option<u64>,
        cue: String,
        at: Vec3,
    },
    /// A speaker's line cut short (audio::Say::Hush).
    Hush { owner: u64 },
    /// An effect on a socket (shared::Effect): its particle system, its socket, the
    /// socket's place and axes, its offset and turn there, its effect_flag bits,
    /// its parameters' names, and the seconds till it is turned off with its clip
    /// (sent with effect_flag::AUTO_OFF; 0 without).
    Effect {
        by: u16,
        system: String,
        socket: String,
        at: Vec3,
        axes: [Vec3; 3],
        offset: Vec3,
        turn: Vec3,
        flags: u8,
        params: Option<Vec<String>>,
        off: f32,
    },
    /// A held effect stopped (its system).
    EffectStop { by: u16, system: String },
}

impl EnemyEvent {
    fn write(&self, w: &mut Writer) {
        match self {
            EnemyEvent::Shot {
                by,
                from,
                to,
                gun,
                flags,
                normal,
            } => {
                w.u8(0);
                w.u16(*by);
                w.vec3(*from);
                w.vec3(*to);
                w.u8(*gun);
                w.u8(*flags | if normal.is_some() { 0x80 } else { 0 });
                if let Some(n) = normal {
                    w.dir16(*n);
                }
            }
            EnemyEvent::Launch {
                by,
                from,
                dir,
                class,
                target,
            } => {
                w.u8(1);
                w.u16(*by);
                w.vec3(*from);
                w.dir16(*dir);
                w.str8(class);
                w.u8(*target);
            }
            EnemyEvent::Throw {
                by,
                class,
                from,
                velocity,
            } => {
                w.u8(2);
                w.u16(*by);
                w.str8(class);
                w.vec3(*from);
                w.vec3(*velocity);
            }
            EnemyEvent::Hurt {
                slot,
                by,
                amount,
                at,
                dir,
                flags,
                momentum,
                shot,
                damage,
            } => {
                w.u8(3);
                w.u8(*slot);
                w.u16(*by);
                w.f32(*amount);
                w.vec3(*at);
                w.dir16(*dir);
                w.u8(*flags
                    | if momentum.is_some() { 0x40 } else { 0 }
                    | if shot.is_some() { 0x80 } else { 0 });
                if let Some(m) = momentum {
                    w.vec3(*m);
                }
                if let Some((death, momentum)) = shot {
                    w.u8(*death);
                    w.f32(*momentum);
                }
                w.u8(*damage);
            }
            EnemyEvent::Barrier { at, amount } => {
                w.u8(4);
                w.vec3(*at);
                w.f32(*amount);
            }
            EnemyEvent::Scanned { slot, by } => {
                w.u8(5);
                w.u8(*slot);
                w.u16(*by);
            }
            EnemyEvent::Sound { by, cue, at } => {
                w.u8(6);
                w.u16(*by);
                w.str8(cue);
                w.vec3(*at);
            }
            EnemyEvent::Line {
                owner,
                cut,
                cue,
                at,
            } => {
                w.u8(7);
                w.u64(*owner);
                match cut {
                    Some(c) => {
                        w.u8(1);
                        w.u64(*c);
                    }
                    None => w.u8(0),
                }
                w.str8(cue);
                w.vec3(*at);
            }
            EnemyEvent::Hush { owner } => {
                w.u8(8);
                w.u64(*owner);
            }
            EnemyEvent::Effect {
                by,
                system,
                socket,
                at,
                axes,
                offset,
                turn,
                flags,
                params,
                off,
            } => {
                w.u8(9);
                w.u16(*by);
                w.str8(system);
                w.str8(socket);
                w.vec3(*at);
                for a in axes {
                    w.dir16(*a);
                }
                let placed = *offset != Vec3::ZERO || *turn != Vec3::ZERO;
                w.u8(*flags
                    | if placed { 0x40 } else { 0 }
                    | if params.is_some() { 0x80 } else { 0 });
                if placed {
                    w.vec3(*offset);
                    w.vec3(*turn);
                }
                if let Some(names) = params {
                    let names = &names[..names.len().min(16)];
                    w.u8(names.len() as u8);
                    for n in names {
                        w.str8(n);
                    }
                }
                if flags & effect_flag::AUTO_OFF != 0 {
                    w.f32(*off);
                }
            }
            EnemyEvent::EffectStop { by, system } => {
                w.u8(10);
                w.u16(*by);
                w.str8(system);
            }
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(match r.u8()? {
            0 => {
                let (by, from, to, gun, flags) = (r.u16()?, r.vec3()?, r.vec3()?, r.u8()?, r.u8()?);
                EnemyEvent::Shot {
                    by,
                    from,
                    to,
                    gun,
                    flags: flags & 0x7f,
                    normal: if flags & 0x80 != 0 {
                        Some(r.dir16()?)
                    } else {
                        None
                    },
                }
            }
            1 => EnemyEvent::Launch {
                by: r.u16()?,
                from: r.vec3()?,
                dir: r.dir16()?,
                class: r.str8()?,
                target: r.u8()?,
            },
            2 => EnemyEvent::Throw {
                by: r.u16()?,
                class: r.str8()?,
                from: r.vec3()?,
                velocity: r.vec3()?,
            },
            3 => {
                let (slot, by, amount, at, dir, flags) =
                    (r.u8()?, r.u16()?, r.f32()?, r.vec3()?, r.dir16()?, r.u8()?);
                let momentum = if flags & 0x40 != 0 {
                    Some(r.vec3()?)
                } else {
                    None
                };
                let shot = if flags & 0x80 != 0 {
                    Some((r.u8()?, r.f32()?))
                } else {
                    None
                };
                EnemyEvent::Hurt {
                    slot,
                    by,
                    amount,
                    at,
                    dir,
                    flags: flags & 0x3f,
                    momentum,
                    shot,
                    damage: r.u8()?,
                }
            }
            4 => EnemyEvent::Barrier {
                at: r.vec3()?,
                amount: r.f32()?,
            },
            5 => EnemyEvent::Scanned {
                slot: r.u8()?,
                by: r.u16()?,
            },
            6 => EnemyEvent::Sound {
                by: r.u16()?,
                cue: r.str8()?,
                at: r.vec3()?,
            },
            7 => {
                let owner = r.u64()?;
                let cut = match r.u8()? {
                    0 => None,
                    _ => Some(r.u64()?),
                };
                EnemyEvent::Line {
                    owner,
                    cut,
                    cue: r.str8()?,
                    at: r.vec3()?,
                }
            }
            8 => EnemyEvent::Hush { owner: r.u64()? },
            9 => {
                let (by, system, socket, at) = (r.u16()?, r.str8()?, r.str8()?, r.vec3()?);
                let axes = [r.dir16()?, r.dir16()?, r.dir16()?];
                let flags = r.u8()?;
                let (offset, turn) = if flags & 0x40 != 0 {
                    (r.vec3()?, r.vec3()?)
                } else {
                    (Vec3::ZERO, Vec3::ZERO)
                };
                let params = if flags & 0x80 != 0 {
                    let n = r.u8()?.min(16);
                    let mut names = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        names.push(r.str8()?);
                    }
                    Some(names)
                } else {
                    None
                };
                let off = if flags & effect_flag::AUTO_OFF != 0 {
                    r.f32()?
                } else {
                    0.0
                };
                EnemyEvent::Effect {
                    by,
                    system,
                    socket,
                    at,
                    axes,
                    offset,
                    turn,
                    flags: flags & 0x3f,
                    params,
                    off,
                }
            }
            10 => EnemyEvent::EffectStop {
                by: r.u16()?,
                system: r.str8()?,
            },
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Game passwords: HMAC-SHA256
// ---------------------------------------------------------------------------

/// SHA-256 (FIPS 180-4).
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    // The message, a 1 bit, zeros up to 8 bytes short of a whole block, and its
    // length in bits.
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for (k, word) in K.iter().zip(w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }
    let mut digest = [0u8; 32];
    for (bytes, v) in digest.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&v.to_be_bytes());
    }
    digest
}

/// HMAC-SHA256 (RFC 2104): `message` signed with `key`.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&sha256(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = block.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(message);
    let mut outer: Vec<u8> = block.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&sha256(&inner));
    sha256(&outer)
}

/// Whether two signatures are the same (comparing every byte, however soon they
/// differ).
fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

// ---------------------------------------------------------------------------
// Packet bytes (little-endian)
// ---------------------------------------------------------------------------

struct Writer(Vec<u8>);

impl Writer {
    fn new(kind: u8) -> Self {
        let mut w = Writer(Vec::with_capacity(256));
        w.0.extend_from_slice(&MAGIC);
        w.u8(kind);
        w
    }

    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    /// A direction (a unit vector's parts, in 32767ths).
    fn dir16(&mut self, v: Vec3) {
        for x in v.to_array() {
            self.i16((x.clamp(-1.0, 1.0) * 32767.0).round() as i16);
        }
    }

    /// A string of up to 255 bytes, after its length.
    fn str8(&mut self, s: &str) {
        let mut end = s.len().min(255);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8);
        self.0.extend_from_slice(&s.as_bytes()[..end]);
    }
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(bytes)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn dir16(&mut self) -> Option<Vec3> {
        let k = 1.0 / 32767.0;
        Some(Vec3::new(
            self.i16()? as f32 * k,
            self.i16()? as f32 * k,
            self.i16()? as f32 * k,
        ))
    }

    /// (A number that isn't one, from a garbled packet, reads as 0.)
    fn f32(&mut self) -> Option<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().ok()?);
        Some(if v.is_finite() { v } else { 0.0 })
    }

    fn vec3(&mut self) -> Option<Vec3> {
        Some(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }

    fn str8(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

/// A match as its host has it, with no network (for main.rs's tests): this PC's
/// player in slot 0 and another player in each of `others`, the match on.
#[cfg(test)]
pub fn host_in_match(others: &[usize]) -> Net {
    let mut net = Net::default();
    net.role = Role::Host { peers: Vec::new() };
    net.me = Some(0);
    net.in_match = true;
    for &slot in std::iter::once(&0).chain(others) {
        net.players[slot] = Some(LobbyPlayer {
            name: format!("Player {slot}"),
            ping_ms: 0,
            score: 0,
            prime: false,
        });
    }
    net
}

/// An enemy state as it comes out of the network (written, then read: for the AI's
/// tests, sync.rs).
#[cfg(test)]
pub fn wired_state(s: &EnemyState) -> EnemyState {
    let mut w = Writer(Vec::new());
    s.write(&mut w);
    EnemyState::read(&mut Reader::new(&w.0)).expect("a state read back")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Held by each test that hosts a game on this PC: they all take the game's
    /// port, so they take turns (run side by side, the second's host failed).
    static PORT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha256_matches_the_standard() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn hmac_sha256_matches_rfc_4231() {
        // Test case 2.
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn the_logs_give_the_date_and_time() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc(1_793_909_225), "2026-11-05 20:07:05 UTC");
        assert_eq!(utc(4_102_444_799), "2099-12-31 23:59:59 UTC");
    }

    /// A host and a player joined to it on this PC, in a match: either may stop
    /// for a while (a slow load, a hitch) and the other waits for it; one silent
    /// for longer is given up, and told so as soon as it is heard again.
    #[test]
    fn a_match_waits_out_a_long_stop() {
        let _port = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let mut host = Net::default();
        host.host("").expect("hosting on port 7777");
        let mut client = Net::default();
        client.join("127.0.0.1", "").expect("joining");
        let dt = 0.05;
        let run = |nets: &mut [&mut Net], secs: f32| {
            for _ in 0..(secs / dt).round() as usize {
                for net in nets.iter_mut() {
                    net.tick(dt);
                    net.send(dt);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        };
        run(&mut [&mut host, &mut client], 1.0);
        host.start_countdown();
        run(&mut [&mut host, &mut client], COUNTDOWN_SECS + 1.0);
        assert!(host.in_match() && client.in_match());
        host.set_local(JazzSnapshot::default());
        client.set_local(JazzSnapshot::default());
        run(&mut [&mut host, &mut client], 1.0);

        // The player stops for 12 s (the host's clock runs on, his stands), then
        // the host for as long: neither gives the other up.
        run(&mut [&mut host], 12.0);
        assert!(host.players[1].is_some(), "the host gave him up");
        run(&mut [&mut host, &mut client], 1.0);
        run(&mut [&mut client], 12.0);
        assert_eq!(client.status(), NetStatus::Match);
        run(&mut [&mut host, &mut client], 1.0);
        assert!(host.players[1].is_some() && client.status() == NetStatus::Match);
        assert_eq!(client.take_failure(), None);

        // Silent for longer than MATCH_TIMEOUT, he is given up; and as soon as he
        // sends again, he is told (not left waiting for the host's silence).
        run(&mut [&mut host], MATCH_TIMEOUT + 1.0);
        assert!(host.players[1].is_none());
        run(&mut [&mut client, &mut host], 0.5);
        assert_eq!(client.status(), NetStatus::Offline);
        assert_eq!(client.take_failure(), Some(Failure::HostLost));
    }

    /// A host and a player joined to it on this PC: the countdown to the match
    /// proper waits for both to have chosen their characters, then runs the same
    /// on both and stays at 0.
    #[test]
    fn the_match_countdown_waits_for_every_player() {
        let _port = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let mut host = Net::default();
        host.host("").expect("hosting on port 7777");
        let mut client = Net::default();
        client.join("127.0.0.1", "").expect("joining");
        let step = |host: &mut Net, client: &mut Net, secs: f32| {
            let dt = 0.05;
            for _ in 0..(secs / dt).round() as usize {
                host.tick(dt);
                host.send(dt);
                std::thread::sleep(std::time::Duration::from_millis(1));
                client.tick(dt);
                client.send(dt);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        step(&mut host, &mut client, 1.0);
        assert_eq!(client.my_slot(), Some(1));
        host.start_countdown();
        step(&mut host, &mut client, COUNTDOWN_SECS + 1.0);
        assert!(host.in_match() && client.in_match());
        host.set_local(JazzSnapshot::default());
        client.set_local(JazzSnapshot::default());

        // Only the host has chosen: no countdown.
        host.set_chosen(true);
        host.set_class(Some(3));
        step(&mut host, &mut client, 2.0);
        assert_eq!(host.match_countdown(), None);
        assert_eq!(client.match_countdown(), None);
        let classes = |net: &Net| {
            net.classes_taken()
                .into_iter()
                .map(|(class, _)| class)
                .collect::<Vec<_>>()
        };
        assert_eq!(classes(&client), vec![3]);
        assert!(classes(&host).is_empty());

        // The other chooses the same class: the host, in the lower slot, keeps it,
        // the other has to choose again, and there is no countdown yet.
        client.set_chosen(true);
        client.set_class(Some(3));
        step(&mut host, &mut client, 2.0);
        assert_eq!(host.match_countdown(), None);
        assert!(client.class_lost() && !host.class_lost());
        assert_eq!(classes(&client), vec![3]);

        // He chooses another: it runs, the same on both.
        client.set_class(Some(0));
        step(&mut host, &mut client, 2.0);
        assert!(!client.class_lost());
        assert_eq!(classes(&host), vec![0]);
        assert_eq!(classes(&client), vec![3]);
        let (h, c) = (host.match_countdown(), client.match_countdown());
        let full = crate::pregame::COUNTDOWN_SECS as f32;
        assert!(
            h.is_some_and(|h| h < full - 1.0 && h > full - 2.1),
            "host {h:?}"
        );
        assert!(
            c.zip(h).is_some_and(|(c, h)| (c - h).abs() < 0.3),
            "client {c:?} host {h:?}"
        );

        // It runs out on both, and stays out.
        step(&mut host, &mut client, full);
        assert_eq!(host.match_countdown(), Some(0.0));
        assert_eq!(client.match_countdown(), Some(0.0));
    }

    /// Split screen's guest (split.rs) takes the host's next slot, and while he is in
    /// the game is closed to players online (one who tries is told it is full); he
    /// can't join once someone online has; and the match proper waits for his
    /// choice of character too.
    #[test]
    fn a_split_screen_guest_takes_a_slot_and_closes_the_game() {
        let _port = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let step = |nets: &mut [&mut Net], secs: f32| {
            let dt = 0.05;
            for _ in 0..(secs / dt).round() as usize {
                for net in nets.iter_mut() {
                    net.tick(dt);
                    net.send(dt);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        };
        let mut host = Net::default();
        // (Only hosting.)
        assert_eq!(host.add_guest("Player 2"), None);
        host.host("").expect("hosting on port 7777");
        assert_eq!(host.add_guest("Player 2"), Some(1));
        assert_eq!(host.guest_slot(), Some(1));
        assert_eq!(host.player_count(), 2);
        assert_eq!(host.add_guest("Player 3"), None, "one guest");

        // Someone online is turned away while he is in.
        let mut client = Net::default();
        client.join("127.0.0.1", "").expect("joining");
        step(&mut [&mut host, &mut client], 1.0);
        assert_eq!(client.status(), NetStatus::Offline);
        assert_eq!(client.take_failure(), Some(Failure::Full));
        assert_eq!(host.player_count(), 2);

        // He leaves: the next one online gets in, and the guest can't come back
        // while he is there.
        host.remove_guest();
        assert_eq!((host.guest_slot(), host.player_count()), (None, 1));
        let mut client = Net::default();
        client.join("127.0.0.1", "").expect("joining");
        step(&mut [&mut host, &mut client], 1.0);
        assert_eq!(client.my_slot(), Some(1));
        assert_eq!(host.add_guest("Player 2"), None);
        client.leave();
        step(&mut [&mut host, &mut client], 1.0);
        assert_eq!(host.player_count(), 1);

        // In the match, the countdown to the match proper waits for him too.
        assert_eq!(host.add_guest("Player 2"), Some(1));
        host.start_countdown();
        assert_eq!(host.add_guest("Player 2"), None);
        step(&mut [&mut host], COUNTDOWN_SECS + 1.0);
        assert!(host.in_match());
        host.set_chosen(true);
        step(&mut [&mut host], 1.0);
        assert_eq!(host.match_countdown(), None);
        host.set_guest_chosen(true);
        step(&mut [&mut host], 1.0);
        assert!(host.match_countdown().is_some());
        // (Nor does he leave mid-match by the lobby's B.)
        host.remove_guest();
        assert_eq!(host.guest_slot(), Some(1));
    }

    #[test]
    fn an_offline_game_hosts_with_no_network() {
        let mut net = Net::default();
        net.set_name("Bumblebee");
        net.host_offline();
        assert!(net.is_host() && net.is_offline_game());
        assert_eq!(net.status(), NetStatus::Lobby);
        assert_eq!(net.my_slot(), Some(0));
        assert_eq!(net.player_count(), 1);
        assert!(!net.has_password());
        assert!(net.socket.is_none(), "no port opened");
        // Split screen's guest can join it, and the match starts as a hosted one's.
        assert_eq!(net.add_guest("Player 2"), Some(1));
        net.start_countdown();
        for _ in 0..((COUNTDOWN_SECS + 1.0) / 0.05) as usize {
            net.tick(0.05);
            net.send(0.05);
        }
        assert!(net.in_match());
        // Leaving it leaves nothing behind.
        net.leave();
        assert!(!net.is_host() && !net.is_offline_game());
        assert_eq!(net.status(), NetStatus::Offline);
        assert_eq!(net.take_failure(), None);
    }

    #[test]
    fn waves_go_out_whole_and_newest_only() {
        let snap = WaveSnap {
            seq: 7,
            wave: 3,
            on: true,
            counting: false,
            countdown: 12,
            markers: true,
            messages: vec![(1, 401), (2, 402), (3, 403), (4, 404), (5, 409)],
            paid: (3, 400),
            cleared: 3,
            purged: 1,
            won: false,
            enemies: vec![EnemySnap {
                id: 41,
                kind: 5,
                at: Vec3::new(1.0, 2.0, 3.0),
                yaw: 1.5,
                seed: 99,
                to: Vec3::new(4.0, 5.0, 6.0),
                leap: true,
                owner: 7,
            }],
        };
        let mut w = Writer(Vec::new());
        snap.write(&mut w);
        let back = WaveSnap::read(&mut Reader::new(&w.0)).expect("read");
        // (The last four messages.)
        assert_eq!(back.messages, snap.messages[1..].to_vec());
        assert_eq!(
            back,
            WaveSnap {
                messages: snap.messages[1..].to_vec(),
                ..snap.clone()
            }
        );
        // A change is numbered; the same waves again aren't.
        let mut net = Net::default();
        net.set_waves(snap.clone());
        assert_eq!(net.waves().map(|w| w.seq), Some(1));
        net.set_waves(snap.clone());
        assert_eq!(net.waves().map(|w| w.seq), Some(1));
        net.set_waves(WaveSnap {
            countdown: 11,
            ..snap
        });
        assert_eq!(net.waves().map(|w| (w.seq, w.countdown)), Some((2, 11)));
        assert!(newer(0, u16::MAX) && newer(5, 4) && !newer(4, 5) && !newer(4, 4));
    }

    #[test]
    fn deaths_go_out_with_their_blows() {
        // (A player's death and an enemy's: how it died, whether it gibs, where it
        // struck and its momentum, which every PC plays alike.)
        for event in [
            JazzEvent::Death {
                death: 3,
                flags: 4 | 1,
                seed: 1234,
                at: Vec3::new(1.0, 2.0, 3.0),
                dir: Vec3::new(0.0, 0.0, -150000.0),
            },
            JazzEvent::EnemyDeath {
                id: 41,
                death: 2,
                flags: 1,
                at: Vec3::new(-4.0, 1.5, 7.25),
                momentum: Vec3::new(70710.0, 0.0, 70710.0),
            },
            // (Its line for the kill feed, the enemy by its number.)
            JazzEvent::KillFeed {
                killer: 1,
                killer_id: 2,
                victim: 2,
                victim_id: 7,
                damage: 12,
                foe: 0xc001,
                head: true,
            },
        ] {
            let mut w = Writer(Vec::new());
            event.write(&mut w);
            let back = JazzEvent::read(&mut Reader::new(&w.0)).expect("read");
            assert_eq!(back, event);
        }
    }

    /// An enemy's state with every part: its pose's clips and shares, its forms
    /// changing (the form it leaves posed too), its plane's turn and its laser sight.
    fn full_state(id: u16) -> EnemyState {
        let pose = |k: f32| Pose {
            clips: vec![
                PoseClip {
                    layer: Layer::Base,
                    clip: 17,
                    weight: 0.75 * k,
                    time: 1.234,
                },
                PoseClip {
                    layer: Layer::Full,
                    clip: 4095,
                    weight: 1.0,
                    time: 0.5,
                },
                PoseClip {
                    layer: Layer::Aim,
                    clip: 3,
                    weight: 2.5,
                    time: 0.0,
                },
            ],
            shares: [0.0, 0.0, 0.0, 0.4, 0.0, 0.999, 0.0, 0.0],
            mirror: 0.25,
            aim_offset: Some((2, Vec2::new(-0.5, 0.75))),
        };
        EnemyState {
            id,
            loc: Vec3::new(-12345.5, 678.25, 90.0),
            vel: Vec3::new(1500.0, -20.0, -980.0),
            rot: [65000, 1200, 64000],
            physics: 10,
            flags: enemy_flag::EQUIPPED | enemy_flag::BOOSTING | enemy_flag::LASER,
            gun: 1,
            scale: 0.25,
            pose: pose(1.0),
            forms: Some(FormsState {
                current: 1,
                target: 0,
                hidden: [false, true],
                fx_allowed: true,
                change: Some((0, 0.4, 1.1)),
                other: Some(pose(0.5)),
            }),
            plane: Some([100, 32768, 65535]),
            laser: Some(LaserState {
                target: Vec3::new(1.0, 2.0, 3.0),
                smoothed: Vec3::new(1.5, 2.5, 3.5),
                spread: 0.3,
                roll: 271.5,
            }),
        }
    }

    #[test]
    fn enemy_states_go_out_close_to_whole() {
        let s = full_state(0xc003);
        let back = wired_state(&s);
        // (What goes as it is: its number, place, turns, physics, flags, gun, forms
        // and plane, laser; its velocity to the unit, its pose's weights, times,
        // shares and mirror to their steps.)
        assert_eq!(
            (
                back.id,
                back.loc,
                back.rot,
                back.physics,
                back.flags,
                back.gun
            ),
            (s.id, s.loc, s.rot, s.physics, s.flags, s.gun)
        );
        assert_eq!((back.plane, back.laser), (s.plane, s.laser));
        assert!(back.vel.distance(s.vel) < 1.0);
        assert!((back.scale - s.scale).abs() < 1e-3);
        let close = |a: &Pose, b: &Pose| {
            assert_eq!(a.clips.len(), b.clips.len());
            for (x, y) in a.clips.iter().zip(&b.clips) {
                assert_eq!((x.layer, x.clip), (y.layer, y.clip));
                assert!((x.weight - y.weight).abs() < 1e-4, "{x:?} {y:?}");
                assert!((x.time - y.time).abs() < 1e-3, "{x:?} {y:?}");
            }
            for (x, y) in a.shares.iter().zip(&b.shares) {
                assert!((x - y).abs() < 1e-4);
            }
            assert!((a.mirror - b.mirror).abs() < 0.005);
            let (pa, aa) = a.aim_offset.unwrap();
            let (pb, ab) = b.aim_offset.unwrap();
            assert!(pa == pb && aa.distance(ab) < 1e-3);
        };
        close(&back.pose, &s.pose);
        let (f, g) = (back.forms.unwrap(), s.forms.unwrap());
        assert_eq!(
            (f.current, f.target, f.hidden, f.fx_allowed, f.change),
            (g.current, g.target, g.hidden, g.fx_allowed, g.change)
        );
        close(&f.other.unwrap(), &g.other.unwrap());
        // One with none of the parts not every enemy has.
        let plain = EnemyState {
            forms: None,
            plane: None,
            laser: None,
            pose: Pose::default(),
            ..s
        };
        let back = wired_state(&plain);
        assert!(back.forms.is_none() && back.plane.is_none() && back.laser.is_none());
        assert!(back.pose.clips.is_empty() && back.pose.aim_offset.is_none());
    }

    #[test]
    fn enemy_events_go_out_whole() {
        let events = [
            EnemyEvent::Shot {
                by: 3,
                from: Vec3::new(1.0, 2.0, 3.0),
                to: Vec3::new(-4.0, 5.5, 6.0),
                gun: 2,
                flags: shot_flag::TRACER | shot_flag::FIRST | shot_flag::TRACED,
                normal: Some(Vec3::Z),
            },
            EnemyEvent::Shot {
                by: 3,
                from: Vec3::ONE,
                to: Vec3::ONE,
                gun: 255,
                flags: shot_flag::FIRST,
                normal: None,
            },
            EnemyEvent::Launch {
                by: 0xc001,
                from: Vec3::new(10.0, 20.0, 30.0),
                dir: Vec3::NEG_Y,
                class: "TnProjectileRocketAI".into(),
                target: 2,
            },
            EnemyEvent::Throw {
                by: 7,
                class: "TnProjectileGrenade".into(),
                from: Vec3::new(1.0, 1.0, 1.0),
                velocity: Vec3::new(300.0, 0.0, 450.0),
            },
            EnemyEvent::Hurt {
                slot: 1,
                by: 9,
                amount: 12.5,
                at: Vec3::new(5.0, 6.0, 7.0),
                dir: Vec3::X,
                flags: hurt_flag::MELEE | hurt_flag::JAM | hurt_flag::GIB,
                momentum: Some(Vec3::new(20000.0, 0.0, 10000.0)),
                shot: Some((4, 30000.0)),
                damage: 22,
            },
            EnemyEvent::Hurt {
                slot: 0,
                by: 0,
                amount: 3.0,
                at: Vec3::ZERO,
                dir: Vec3::NEG_X,
                flags: 0,
                momentum: None,
                shot: None,
                damage: 0,
            },
            EnemyEvent::Barrier {
                at: Vec3::new(-3.0, 0.5, 9.0),
                amount: 40.0,
            },
            EnemyEvent::Scanned { slot: 3, by: 12 },
            EnemyEvent::Sound {
                by: 4,
                cue: "BL_AI_SOLDIER.FOOTSTEP".into(),
                at: Vec3::new(1.0, 2.0, 3.0),
            },
            EnemyEvent::Line {
                owner: 0xDEAD_BEEF_0123_4567,
                cut: Some(42),
                cue: "BL_DX_SOLDIER.SC000001".into(),
                at: Vec3::Y,
            },
            EnemyEvent::Line {
                owner: 1,
                cut: None,
                cue: String::new(),
                at: Vec3::ZERO,
            },
            EnemyEvent::Hush { owner: u64::MAX },
            EnemyEvent::Effect {
                by: 5,
                system: "Guardian_Charge_FX".into(),
                socket: "MuzzleFlash".into(),
                at: Vec3::new(1.0, 2.0, 3.0),
                axes: [Vec3::X, Vec3::NEG_Z, Vec3::Y],
                offset: Vec3::new(0.0, 5.0, 0.0),
                turn: Vec3::new(0.0, 16384.0, 0.0),
                flags: effect_flag::ATTACHED,
                params: Some(vec!["EnergonColor".into(), "@team".into()]),
                off: 0.0,
            },
            EnemyEvent::Effect {
                by: 7,
                system: "Leaper_Boosters_LowBurn_FX".into(),
                socket: "FX_Booster_Back_R".into(),
                at: Vec3::new(4.0, 5.0, 6.0),
                axes: [Vec3::X, Vec3::Y, Vec3::Z],
                offset: Vec3::ZERO,
                turn: Vec3::ZERO,
                flags: effect_flag::ATTACHED | effect_flag::AUTO_OFF,
                params: None,
                off: 0.5733,
            },
            EnemyEvent::Effect {
                by: 5,
                system: "Muzzle_FX".into(),
                socket: String::new(),
                at: Vec3::ZERO,
                axes: [Vec3::X, Vec3::Y, Vec3::Z],
                offset: Vec3::ZERO,
                turn: Vec3::ZERO,
                flags: effect_flag::ATTACHED | effect_flag::ONLY_ONCE,
                params: None,
                off: 0.0,
            },
            EnemyEvent::EffectStop {
                by: 5,
                system: "Guardian_Charge_FX".into(),
            },
        ];
        for event in events {
            let mut w = Writer(Vec::new());
            event.write(&mut w);
            let back = EnemyEvent::read(&mut Reader::new(&w.0)).expect("read");
            assert_eq!(back, event);
        }
    }

    /// A host and a player joined to it on this PC, in a match: the host's enemies
    /// (more than one packet holds) and their events reach the player, each event
    /// once (those sent again too), with the host's clock.
    #[test]
    fn the_hosts_enemies_reach_its_players() {
        let _port = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let mut host = Net::default();
        host.host("").expect("hosting on port 7777");
        let mut client = Net::default();
        client.join("127.0.0.1", "").expect("joining");
        let step = |host: &mut Net, client: &mut Net, secs: f32| {
            let dt = 0.02;
            for _ in 0..(secs / dt).round() as usize {
                host.tick(dt);
                host.send(dt);
                std::thread::sleep(std::time::Duration::from_millis(1));
                client.tick(dt);
                client.send(dt);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        step(&mut host, &mut client, 0.5);
        host.start_countdown();
        step(&mut host, &mut client, COUNTDOWN_SECS + 0.5);
        assert!(host.in_match() && client.in_match());
        let states: Vec<EnemyState> = (1..=30).map(full_state).collect();
        host.set_enemy_states(states);
        host.push_enemy_event(EnemyEvent::Hush { owner: 1 }, true);
        host.push_enemy_event(EnemyEvent::Hush { owner: 2 }, false);
        host.push_enemy_event(EnemyEvent::Hush { owner: 3 }, true);
        step(&mut host, &mut client, 0.6);
        let got = client.take_enemy_states();
        let ids: std::collections::BTreeSet<u16> = got.iter().map(|(_, s)| s.id).collect();
        assert_eq!(ids, (1..=30).collect(), "every enemy came");
        let owners: Vec<u64> = client
            .take_enemy_events()
            .into_iter()
            .map(|(_, e)| match e {
                EnemyEvent::Hush { owner } => owner,
                _ => 0,
            })
            .collect();
        assert_eq!(
            owners,
            vec![1, 3, 2],
            "each event once, those sent again first"
        );
        let (h, c) = (
            host.sync_clock(),
            client.host_clock().expect("the host's clock"),
        );
        assert!((h - c).abs() < 0.1, "host {h} client's idea of it {c}");
        // (A packet's states are its own time's.)
        let (newest, _) = got.last().unwrap();
        assert!((h - newest).abs() < 0.2);
    }

    #[test]
    fn passwords_are_cleaned() {
        assert_eq!(clean_password(""), None);
        assert_eq!(clean_password("   "), None);
        assert_eq!(clean_password(" open sesame "), Some("open sesame".into()));
        assert_eq!(
            clean_password(&"x".repeat(40)).map(|p| p.len()),
            Some(PASSWORD_MAX)
        );
    }
}
