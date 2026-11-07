"""The player installer's game-files builder: a small window that builds the
demo's game files from the player's own copy of Transformers: Fall of Cybertron
(tools\\extract_game.py, the same build as "extract assets.bat"), showing how far
it has got. Cybertron Escalation Setup (tools\\installer\\foc_esc.iss) installs it
with the demo, its own Python and UE Viewer, and runs it as it finishes; the demo
runs it (--play) when its game files aren't built yet, or an update needs them
built again; the Start menu has it as "Build the game files".

What it does, in an installed copy (<app> is the install folder):
- finds the game (its folder as last found or chosen, <app>\\game_folder.txt, or
  in Steam's libraries), or asks for its folder;
- downloads ffmpeg (a video and sound tool the build needs: GPL, so it is
  fetched from its publisher rather than shipped with the demo) into
  <app>\\tools\\ffmpeg, once;
- runs extract_game.py into <app>\\assets (work files in <app>\\extract_work), its
  output in <app>\\extract_work\\builder.log, reading its progress from that output;
- when all of it is built, writes <app>\\game_files.txt: the id of the build tools
  it was built with (tools\\installer\\pipeline.txt, made by make_installer.py). The
  demo starts as usual while the two are the same.

Closing it stops the build; the next run carries on from the step it stopped in.
An update whose build tools differ builds everything again.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLS = os.path.dirname(HERE)
APP = os.path.dirname(TOOLS)

TITLE = "Cybertron Escalation"
# The demo's game files take about 2.4 GB, its work files up to about 2.5 GB more
# while it builds, and ffmpeg's download 0.1 GB.
NEEDED_BYTES = 6 * 1024 ** 3

# ffmpeg: gyan.dev's release "essentials" build (64-bit, static, with libx264 and
# libvorbis, which the menus' movies and the sounds need), with its SHA-256; its
# GitHub mirror if gyan.dev can't be reached.
FFMPEG_URL = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
FFMPEG_MIRROR_API = "https://api.github.com/repos/GyanD/codexffmpeg/releases/latest"
USER_AGENT = "CybertronEscalationSetup/1.0"

# How long each step takes, roughly, in minutes (the whole build took 77 minutes
# on a slow two-core PC: the menus' movies 20, the sounds 10, the doors 8). They
# share out the progress bar; the time left is worked out from how fast this PC
# has gone so far.
STEP_MINUTES = {
    "menu": 20, "sounds": 10, "doors": 8,
    "map_downfall": 3, "map_oblivion": 3, "map_ancients": 3, "map_ignition": 3,
    "class_infiltrator": 2, "class_destroyer": 2, "class_titan": 2, "class_scientist": 2,
    "enemies": 2,
}

CREATE_NO_WINDOW = 0x08000000
# SetThreadExecutionState: keep the PC from going to sleep while it builds.
ES_CONTINUOUS = 0x80000000
ES_SYSTEM_REQUIRED = 0x00000001


class Paths:
    """Where an installed copy keeps things (`app`: the install folder)."""

    def __init__(self, app=APP):
        self.app = app
        self.tools = os.path.join(app, "tools")
        self.pipeline = os.path.join(self.tools, "installer", "pipeline.txt")
        self.marker = os.path.join(app, "game_files.txt")
        self.game_file = os.path.join(app, "game_folder.txt")
        self.assets = os.path.join(app, "assets")
        self.work = os.path.join(app, "extract_work")
        self.work_pipeline = os.path.join(self.work, "pipeline.txt")
        self.done = os.path.join(self.work, "done.txt")
        self.log = os.path.join(self.work, "builder.log")
        self.lock = os.path.join(self.work, "builder.lock")
        self.ffmpeg_dir = os.path.join(self.tools, "ffmpeg")
        self.ffmpeg = os.path.join(self.ffmpeg_dir, "ffmpeg.exe" if os.name == "nt" else "ffmpeg")
        self.exe = os.path.join(app, "foc_esc.exe" if os.name == "nt" else "foc_esc")
        self.icon = os.path.join(app, "icon", "foc_esc.ico")


def read_text(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


def write_text(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text + "\n")


def build_state(p):
    """What there is to do: "ready" (built with these tools), "resume" (carry on
    with a build of these tools, or start one) or "redo" (built, or part built,
    with other tools: build everything again)."""
    want = read_text(p.pipeline)
    if want and read_text(p.marker) == want:
        return "ready"
    work = read_text(p.work_pipeline)
    if os.path.exists(p.done) and work != want:
        return "redo"
    if not os.path.exists(p.done) and os.path.exists(p.marker):
        return "redo"
    return "resume"


# ---------------------------------------------------------------------------
# Finding the game
# ---------------------------------------------------------------------------

def valid_game(folder):
    return bool(folder) and os.path.isdir(os.path.join(folder, "TransGame", "CookedPC"))


def find_game(p):
    """The game's folder: as last found or chosen, or in a Steam library."""
    saved = read_text(p.game_file)
    if valid_game(saved):
        return os.path.abspath(saved)
    sys.path.insert(0, p.tools)
    try:
        import extract_game
        return extract_game.find_game(None)
    except Exception:
        return None
    finally:
        sys.path.remove(p.tools)


def game_from_choice(folder):
    """The game's folder from one the player chose: it, or the game's folder
    within it, or the one it's in (TransGame or CookedPC chosen)."""
    if not folder:
        return None
    folder = os.path.abspath(folder)
    for c in [folder, os.path.join(folder, "Transformers Fall of Cybertron"),
              os.path.dirname(folder), os.path.dirname(os.path.dirname(folder))]:
        if valid_game(c):
            return c
    return None


# ---------------------------------------------------------------------------
# ffmpeg
# ---------------------------------------------------------------------------

def ffmpeg_works(path):
    """`path` is an ffmpeg with the encoders the build uses."""
    if not path or not os.path.isfile(path):
        return False
    try:
        out = subprocess.run([path, "-hide_banner", "-encoders"], capture_output=True, timeout=60,
                             creationflags=CREATE_NO_WINDOW if os.name == "nt" else 0)
    except (OSError, subprocess.SubprocessError):
        return False
    text = out.stdout.decode("utf-8", "replace")
    return out.returncode == 0 and "libx264" in text and "libvorbis" in text


def fetch(url, timeout=60):
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    return urllib.request.urlopen(req, timeout=timeout)


def download(url, dest, progress, stop):
    """Downloads `url` to `dest`, calling progress(bytes, total); returns the
    file's SHA-256. `stop` (a threading.Event) set stops it."""
    h = hashlib.sha256()
    with fetch(url) as r, open(dest, "wb") as out:
        total = int(r.headers.get("Content-Length") or 0)
        got = 0
        while True:
            if stop.is_set():
                raise InterruptedError("stopped")
            chunk = r.read(1 << 16)
            if not chunk:
                break
            out.write(chunk)
            h.update(chunk)
            got += len(chunk)
            progress(got, total)
    return h.hexdigest()


def ffmpeg_sources():
    """(zip URL, its SHA-256 or None) to try, in order."""
    out = []
    try:
        with fetch(FFMPEG_URL + ".sha256", timeout=30) as r:
            sha = r.read().decode("ascii", "replace").split()[0].strip().lower()
        out.append((FFMPEG_URL, sha if len(sha) == 64 else None))
    except Exception:
        out.append((FFMPEG_URL, None))
    try:
        with fetch(FFMPEG_MIRROR_API, timeout=30) as r:
            release = json.load(r)
        for a in release.get("assets", []):
            if a.get("name", "").endswith("essentials_build.zip"):
                out.append((a["browser_download_url"], None))
                break
    except Exception:
        pass
    return out


def extract_ffmpeg(zip_path, dest):
    """Takes ffmpeg.exe out of a build's zip (<build>/bin/ffmpeg.exe) to `dest`."""
    with zipfile.ZipFile(zip_path) as z:
        names = [n for n in z.namelist() if n.replace("\\", "/").endswith("/bin/ffmpeg.exe")]
        if not names:
            raise ValueError("no bin/ffmpeg.exe in the download")
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        tmp = dest + ".part"
        with z.open(names[0]) as src, open(tmp, "wb") as out:
            shutil.copyfileobj(src, out)
        os.replace(tmp, dest)


def ensure_ffmpeg(p, progress, stop, log):
    """ffmpeg for the build: its own copy (downloaded the first time), or one on the
    PATH if the download fails. Returns its folder, or raises."""
    if ffmpeg_works(p.ffmpeg):
        return p.ffmpeg_dir
    os.makedirs(p.work, exist_ok=True)
    zip_path = os.path.join(p.work, "ffmpeg.zip")
    errors = []
    for url, sha in ffmpeg_sources():
        try:
            log(f"downloading {url}")
            got = download(url, zip_path, progress, stop)
            if sha and got != sha:
                raise ValueError(f"SHA-256 {got}, expected {sha}")
            extract_ffmpeg(zip_path, p.ffmpeg)
            if ffmpeg_works(p.ffmpeg):
                return p.ffmpeg_dir
            raise ValueError("the downloaded ffmpeg doesn't run")
        except InterruptedError:
            raise
        except Exception as e:
            errors.append(f"{url}: {e}")
            log(f"  failed: {e}")
        finally:
            if os.path.exists(zip_path):
                os.remove(zip_path)
    on_path = shutil.which("ffmpeg")
    if ffmpeg_works(on_path):
        log(f"using the ffmpeg on the PATH: {on_path}")
        return os.path.dirname(on_path)
    raise RuntimeError("; ".join(errors) or "no ffmpeg to download")


# ---------------------------------------------------------------------------
# The build's progress (extract_game.py's output)
# ---------------------------------------------------------------------------

def parse_line(line):
    """An event from a line of extract_game.py's output: ("done", step) for a step
    built before, ("start", step), ("ok", step), ("failed", step, log),
    ("finished",), or None."""
    s = line.rstrip("\r\n")
    if s.startswith("  done   "):
        return ("done", s[9:].strip())
    if s.startswith("  ...    "):
        return ("start", s[9:].split(":", 1)[0].strip())
    if s.startswith("  ok     "):
        return ("ok", s[9:].split(" (", 1)[0].strip())
    if s.startswith("  FAILED "):
        rest = s[9:]
        name = rest.split(" after ", 1)[0].strip()
        log = rest.split(": see ", 1)[1].strip() if ": see " in rest else ""
        return ("failed", name, log)
    if s.startswith("All done"):
        return ("finished",)
    return None


class Progress:
    """How far the build has got: the steps (name, what) in order, those done, and
    the one running; each step's share of the bar by STEP_MINUTES."""

    def __init__(self, steps):
        self.steps = list(steps)
        self.what = dict(self.steps)
        self.weight = {n: STEP_MINUTES.get(n, 1.0) for n, _ in self.steps}
        self.total = sum(self.weight.values()) or 1.0
        self.done = set()
        self.current = None
        self.started = None
        # (The weight done before this run: the time left counts this run's pace.)
        self.done_before = 0.0

    def event(self, ev, now):
        kind = ev[0]
        if kind == "done":
            self.done.add(ev[1])
            self.done_before = self.done_weight()
        elif kind == "start":
            self.current = ev[1]
            if self.started is None:
                self.started = now
                self.done_before = self.done_weight()
        elif kind == "ok":
            self.done.add(ev[1])
            if self.current == ev[1]:
                self.current = None

    def done_weight(self):
        return sum(self.weight.get(n, 1.0) for n in self.done)

    def fraction(self):
        return min(1.0, self.done_weight() / self.total)

    def step_number(self):
        """(the running step's number, how many), 1-based."""
        names = [n for n, _ in self.steps]
        if self.current in names:
            return names.index(self.current) + 1, len(names)
        return len(self.done), len(names)

    def minutes_left(self, now):
        """Minutes to go, from this run's pace, once a fair part has been built
        (None before)."""
        if self.started is None:
            return None
        built = self.done_weight() - self.done_before
        if built < 3.0 or now - self.started < 60:
            return None
        per_weight = (now - self.started) / built
        return max(1, round((self.total - self.done_weight()) * per_weight / 60))


def load_steps(p):
    """The build's steps (name, what), from extract_game.py."""
    sys.path.insert(0, p.tools)
    try:
        import extract_game
        return [(s.name, s.what) for s in extract_game.STEPS]
    finally:
        sys.path.remove(p.tools)


def describe(what):
    """A step's description for the window: extract_game's, without its details
    in brackets, with a capital."""
    short = re.sub(r"\s*\([^)]*\)", "", what).strip()
    return short[:1].upper() + short[1:]


def time_left_text(minutes):
    if minutes is None:
        return "Working out how long it will take..."
    if minutes >= 90:
        return f"About {round(minutes / 60)} hours left" if minutes >= 150 else "About an hour and a half left"
    if minutes >= 55:
        return "About an hour left"
    if minutes <= 1:
        return "About a minute left"
    return f"About {minutes} minutes left"


# ---------------------------------------------------------------------------
# Running the build
# ---------------------------------------------------------------------------

class Build:
    """extract_game.py running, its output going to the log and, as events, to
    `on_event` (called from a reader thread)."""

    def __init__(self, p, game, ffmpeg_dir, redo, on_event, on_exit):
        self.p = p
        python = sys.executable
        if os.path.basename(python).lower() == "pythonw.exe":
            console = os.path.join(os.path.dirname(python), "python.exe")
            if os.path.exists(console):
                python = console
        cmd = [python, "-u", os.path.join(p.tools, "extract_game.py"), "--game", game,
               "--assets", p.assets, "--work", p.work]
        if redo:
            cmd.append("--redo")
        env = {k: v for k, v in os.environ.items() if k not in ("PYTHONHOME", "PYTHONPATH")}
        env["PATH"] = ffmpeg_dir + os.pathsep + env.get("PATH", "")
        env.update(PYTHONIOENCODING="utf-8", PYTHONUTF8="1", PYTHONUNBUFFERED="1", PYTHONNOUSERSITE="1")
        os.makedirs(p.work, exist_ok=True)
        self.log = open(p.log, "a", encoding="utf-8")
        self.log.write(f"\n=== {time.strftime('%Y-%m-%d %H:%M:%S')} " + " ".join(cmd) + "\n")
        self.log.flush()
        self.proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     cwd=p.tools, env=env, encoding="utf-8", errors="replace",
                                     creationflags=CREATE_NO_WINDOW if os.name == "nt" else 0)
        self.on_event = on_event
        self.on_exit = on_exit
        self.last_lines = []
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.log.write(line)
            self.log.flush()
            if line.strip():
                self.last_lines = (self.last_lines + [line.rstrip()])[-8:]
            ev = parse_line(line)
            if ev:
                self.on_event(ev)
        code = self.proc.wait()
        self.log.write(f"=== exit {code}\n")
        self.log.close()
        self.on_exit(code)

    def stop(self):
        if self.proc.poll() is not None:
            return
        if os.name == "nt":
            subprocess.run(["taskkill", "/PID", str(self.proc.pid), "/T", "/F"], capture_output=True,
                           creationflags=CREATE_NO_WINDOW)
        else:
            self.proc.kill()


def keep_awake(on):
    if os.name != "nt":
        return
    try:
        import ctypes
        flags = ES_CONTINUOUS | (ES_SYSTEM_REQUIRED if on else 0)
        ctypes.windll.kernel32.SetThreadExecutionState(flags)
    except Exception:
        pass


class Lock:
    """One builder at a time: an exclusive lock on extract_work\\builder.lock."""

    def __init__(self, path):
        self.path = path
        self.f = None

    def acquire(self):
        os.makedirs(os.path.dirname(self.path), exist_ok=True)
        self.f = open(self.path, "a+")
        try:
            if os.name == "nt":
                import msvcrt
                self.f.seek(0)
                msvcrt.locking(self.f.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            return True
        except OSError:
            self.f.close()
            self.f = None
            return False

    def release(self):
        if self.f:
            self.f.close()
            self.f = None


def launch_game(p):
    subprocess.Popen([p.exe], cwd=p.app, close_fds=True)


def open_file(path):
    if os.name == "nt":
        os.startfile(path)  # noqa: S606 (the player's own log)


# ---------------------------------------------------------------------------
# The window
# ---------------------------------------------------------------------------

# The window's width, in points (so it scales with the display's DPI).
WIDE = "405p"
NARROW = "330p"

INTRO = ("The demo builds its maps, characters, sounds and menus from your own copy of "
         "Transformers: Fall of Cybertron. This takes about an hour (longer on an older PC) "
         "and needs about 6 GB of free space. You can use your PC meanwhile. Closing this "
         "window stops it; next time it carries on from where it stopped.")


class App:
    """The builder's window. `tk`, `ttk`, `filedialog` and `messagebox` are tkinter's
    (passed in, so the tests can give stand-ins)."""

    def __init__(self, tk, ttk, filedialog, messagebox, p, play=False, after_setup=False):
        self.tk, self.ttk, self.filedialog, self.messagebox = tk, ttk, filedialog, messagebox
        self.p = p
        self.play = play
        self.after_setup = after_setup
        self.build = None
        self.stop_download = threading.Event()
        self.events = []
        self.events_lock = threading.Lock()
        self.phase = "idle"
        self.failed_log = None
        # (Build everything again: asked for, or the tools changed.)
        self.redo = False
        self.progress = Progress(load_steps(p))
        self.game = find_game(p)

        root = self.root = tk.Tk()
        root.title(TITLE)
        root.resizable(False, False)
        if os.path.exists(p.icon):
            try:
                root.iconbitmap(p.icon)
            except Exception:
                pass
        root.protocol("WM_DELETE_WINDOW", self.close)
        frame = ttk.Frame(root, padding=18)
        frame.grid(sticky="nsew")
        self.heading = ttk.Label(frame, text="Building the game files", font=("Segoe UI", 14, "bold"))
        self.heading.grid(row=0, column=0, columnspan=2, sticky="w")
        self.intro = ttk.Label(frame, text=INTRO, wraplength=WIDE, justify="left")
        self.intro.grid(row=1, column=0, columnspan=2, sticky="w", pady=(8, 12))
        self.game_label = ttk.Label(frame, text="", wraplength=NARROW, justify="left")
        self.game_label.grid(row=2, column=0, sticky="w")
        self.change_button = ttk.Button(frame, text="Change...", command=self.choose_game)
        self.change_button.grid(row=2, column=1, sticky="e")
        self.bar = ttk.Progressbar(frame, orient="horizontal", length=WIDE, mode="determinate", maximum=1000)
        self.bar.grid(row=3, column=0, columnspan=2, pady=(14, 6), sticky="we")
        self.status = ttk.Label(frame, text="", wraplength=WIDE, justify="left")
        self.status.grid(row=4, column=0, columnspan=2, sticky="w")
        self.detail = ttk.Label(frame, text="", wraplength=WIDE, justify="left")
        self.detail.grid(row=5, column=0, columnspan=2, sticky="w", pady=(2, 0))
        buttons = ttk.Frame(frame)
        buttons.grid(row=6, column=0, columnspan=2, sticky="e", pady=(16, 0))
        self.main_button = ttk.Button(buttons, text="Start", command=self.start)
        self.main_button.grid(row=0, column=0, padx=(0, 8))
        self.second_button = ttk.Button(buttons, text="Show the log", command=self.show_log)
        self.second_button.grid(row=0, column=1, padx=(0, 8))
        self.close_button = ttk.Button(buttons, text="Close", command=self.close)
        self.close_button.grid(row=0, column=2)
        try:
            # (In the middle of the screen, in front: Setup's window has just closed.)
            root.eval("tk::PlaceWindow . center")
            root.lift()
            root.attributes("-topmost", True)
            root.after(500, lambda: root.attributes("-topmost", False))
        except Exception:
            pass

        self.lock = Lock(p.lock)
        if not self.lock.acquire():
            self.show("Already building", "The game files are being built in another window.",
                      main=None, second=None)
            self.phase = "other"
            return
        self.show_game()
        state = build_state(p)
        if state == "ready" and not self.after_setup:
            if self.play:
                launch_game(p)
                self.lock.release()
                root.after(100, root.destroy)
                return
            self.phase = "ready"
            self.bar["value"] = 1000
            self.show("The game files are built", "They are up to date: the demo is ready to play.",
                      main=("Play", self.play_now), second=("Build them again", self.rebuild))
        elif state == "ready":
            self.phase = "ready"
            self.bar["value"] = 1000
            self.show("The game files are built", "They are up to date: the demo is ready to play.",
                      main=("Play", self.play_now), second=None)
        elif not self.game:
            self.show("Where is Fall of Cybertron?",
                      "Transformers: Fall of Cybertron wasn't found in Steam's folders. Choose the "
                      "folder it's installed in (the one with the TransGame folder in it).",
                      main=("Choose its folder...", self.choose_game), second=None)
        else:
            # (Started at once: Setup, or the demo, asked for it.)
            root.after(300, self.start)
        root.after(200, self.pump)

    # -- what the window says ------------------------------------------------

    def show(self, heading, status, main=None, second=None, detail=""):
        self.heading["text"] = heading
        self.status["text"] = status
        self.detail["text"] = detail
        for button, spec in ((self.main_button, main), (self.second_button, second)):
            if spec is None:
                button.grid_remove()
            else:
                button["text"], button["command"] = spec
                button.grid()

    def show_game(self):
        self.game_label["text"] = ("Game folder: " + self.game) if self.game else "Game folder: not found"

    # -- actions -------------------------------------------------------------

    def choose_game(self):
        if self.phase in ("download", "build"):
            return
        folder = self.filedialog.askdirectory(
            title="Choose Transformers: Fall of Cybertron's folder", mustexist=True,
            initialdir=self.game or os.path.expanduser("~"))
        if not folder:
            return
        game = game_from_choice(folder)
        if not game:
            self.messagebox.showerror(
                TITLE, "That folder doesn't have Fall of Cybertron's game files in it (its "
                "TransGame\\CookedPC folder). Choose the folder the game is installed in.")
            return
        self.game = game
        write_text(self.p.game_file, game)
        self.show_game()
        if self.phase in ("idle", "failed"):
            self.start()

    def start(self):
        if self.phase in ("download", "build", "other"):
            return
        if not valid_game(self.game):
            self.game = None
            self.show_game()
            self.choose_game()
            return
        write_text(self.p.game_file, self.game)
        free = shutil.disk_usage(self.p.app).free
        if not os.path.exists(self.p.done) and free < NEEDED_BYTES:
            self.show("Not enough space", f"Building the game files needs about 6 GB of free space on "
                      f"this drive, and it has {free / 1024 ** 3:.1f} GB. Free some space, then "
                      "choose Try again.", main=("Try again", self.start), second=None)
            self.phase = "failed"
            return
        if build_state(self.p) == "redo":
            self.redo = True
        self.phase = "download"
        self.bar["value"] = 0
        self.show("Building the game files", "Getting ffmpeg ready (a tool for the movies and sounds)...")
        keep_awake(True)
        threading.Thread(target=self._download, daemon=True).start()

    def _download(self):
        def progress(got, total):
            text = f"Downloading ffmpeg (a tool for the movies and sounds): {got / 1e6:.0f}"
            text += f" of {total / 1e6:.0f} MB" if total else " MB"
            self.post(("status", text, got / total if total else None))

        try:
            folder = ensure_ffmpeg(self.p, progress, self.stop_download, self.log)
            self.post(("ffmpeg", folder))
        except InterruptedError:
            pass
        except Exception as e:
            self.log(f"ffmpeg: {e}")
            self.post(("ffmpeg_failed", str(e)))

    def log(self, text):
        try:
            os.makedirs(self.p.work, exist_ok=True)
            with open(self.p.log, "a", encoding="utf-8") as f:
                f.write(f"{time.strftime('%H:%M:%S')} {text}\n")
        except OSError:
            pass

    def run_build(self, ffmpeg_dir):
        redo = self.redo
        # (A fresh count: the build lists the steps it has already done.)
        self.progress = Progress(self.progress.steps)
        self.failed_log = None
        write_text(self.p.work_pipeline, read_text(self.p.pipeline))
        if os.path.exists(self.p.marker):
            os.remove(self.p.marker)
        self.phase = "build"
        self.show("Building the game files", "Starting...", main=None, second=("Show the log", self.show_log))
        try:
            self.build = Build(self.p, self.game, ffmpeg_dir, redo, lambda ev: self.post(("event", ev)),
                               lambda code: self.post(("exit", code)))
        except Exception as e:
            self.log(f"couldn't start the build: {e}")
            self.build = None
            self.finished(-1)
            return
        self.redo = False

    def rebuild(self):
        if self.messagebox.askyesno(TITLE, "Build all the game files again? It takes about an hour."):
            self.redo = True
            self.phase = "idle"
            self.start()

    def play_now(self):
        launch_game(self.p)
        self.lock.release()
        self.root.destroy()

    def show_log(self):
        path = self.failed_log if self.failed_log and os.path.exists(self.failed_log) else self.p.log
        if os.path.exists(path):
            open_file(path)

    def close(self):
        if self.phase in ("download", "build"):
            if not self.messagebox.askyesno(
                    TITLE, "Stop building the game files? Next time it carries on from where it stopped."):
                return
            self.stop_download.set()
            if self.build:
                self.build.stop()
        keep_awake(False)
        self.lock.release()
        self.root.destroy()

    # -- events from the worker threads ----------------------------------------

    def post(self, item):
        with self.events_lock:
            self.events.append(item)

    def pump(self):
        with self.events_lock:
            items, self.events = self.events, []
        for item in items:
            self.handle(item)
        try:
            self.root.after(200, self.pump)
        except Exception:
            pass

    def handle(self, item):
        kind = item[0]
        now = time.time()
        if kind == "status":
            self.status["text"] = item[1]
            if item[2] is not None:
                self.bar["value"] = int(item[2] * 1000)
        elif kind == "ffmpeg":
            self.run_build(item[1])
        elif kind == "ffmpeg_failed":
            keep_awake(False)
            self.phase = "failed"
            self.show("Couldn't get ffmpeg",
                      "ffmpeg (a tool the build needs for the movies and sounds) couldn't be downloaded. "
                      "Check that this PC is online, then choose Try again.",
                      main=("Try again", self.start), second=("Show the log", self.show_log))
        elif kind == "event":
            ev = item[1]
            self.progress.event(ev, now)
            if ev[0] == "failed":
                self.failed_log = ev[2] or None
            self.update_progress(now)
        elif kind == "exit":
            self.finished(item[1])

    def update_progress(self, now):
        pr = self.progress
        self.bar["value"] = int(pr.fraction() * 1000)
        if pr.current:
            n, total = pr.step_number()
            self.status["text"] = f"Step {n} of {total}: {describe(pr.what.get(pr.current, pr.current))}"
            self.detail["text"] = time_left_text(pr.minutes_left(now))

    def finished(self, code):
        keep_awake(False)
        last = self.build.last_lines[-3:] if self.build else []
        self.build = None
        all_done = code == 0 and all(n in self.progress.done for n, _ in self.progress.steps)
        if all_done:
            write_text(self.p.marker, read_text(self.p.pipeline))
            self.phase = "done"
            self.bar["value"] = 1000
            self.show("All done!", "The game files are built: the demo is ready to play.",
                      main=("Play", self.play_now), second=None)
            if self.play:
                self.root.after(1500, self.play_now)
            return
        self.phase = "failed"
        step = self.progress.current
        what = describe(self.progress.what.get(step, step)) if step else None
        lines = "\n".join(last)
        self.show("Something went wrong",
                  (f"Building \"{what}\" didn't work. " if what else "The build stopped. ")
                  + "Choose Try again to carry on from there. If it goes wrong again, send the log "
                  "(Show the log) to whoever gave you the demo.",
                  main=("Try again", self.start), second=("Show the log", self.show_log), detail=lines)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    p = Paths()
    if sys.stderr is None:
        # (pythonw.exe has no console: what Python would print there, an error in
        # the window's code say, goes to the log.)
        try:
            os.makedirs(p.work, exist_ok=True)
            sys.stdout = sys.stderr = open(p.log, "a", encoding="utf-8", buffering=1)
        except OSError:
            pass
    if os.name == "nt":
        try:
            import ctypes
            ctypes.windll.shcore.SetProcessDpiAwareness(1)
        except Exception:
            pass
    import tkinter
    from tkinter import filedialog, messagebox, ttk
    if "--self-test" in argv:
        # (make_installer.py: the bundled Python runs tkinter and reads the steps.)
        root = tkinter.Tk()
        root.withdraw()
        ttk.Progressbar(root)
        root.update()
        root.destroy()
        print(f"builder ok: {len(load_steps(p))} steps, Tk {tkinter.TkVersion}")
        return 0
    app = App(tkinter, ttk, filedialog, messagebox, p, play="--play" in argv,
              after_setup="--after-setup" in argv)
    app.root.mainloop()
    return 0


if __name__ == "__main__":
    sys.exit(main())
