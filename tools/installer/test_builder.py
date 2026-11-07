"""Tests for builder.py, with stand-ins for tkinter and for extract_game.py (a fake
build that prints what the real one prints): python -m unittest test_builder
(in tools\\installer)."""
import os
import sys
import tempfile
import textwrap
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import builder  # noqa: E402

# A stand-in extract_game.py: three steps; FAKE_FAIL=<step> fails that step. It
# notes its arguments in args.txt, and keeps done.txt as the real one does.
FAKE_EXTRACT = textwrap.dedent('''
    import os, sys
    class Step:
        def __init__(self, name, what):
            self.name, self.what = name, what
    STEPS = [Step("map_downfall", "the Downfall map (meshes, textures)"),
             Step("menu", "the menus (pictures, fonts)"),
             Step("sounds", "every sound (last: it reads what the others wrote)")]
    def find_game(given):
        return os.environ.get("FAKE_GAME") or None
    if __name__ == "__main__":
        args = sys.argv[1:]
        work = args[args.index("--work") + 1]
        os.makedirs(work, exist_ok=True)
        open(os.path.join(os.path.dirname(work), "args.txt"), "w").write(" ".join(args))
        done_path = os.path.join(work, "done.txt")
        done = [] if "--redo" in args or not os.path.exists(done_path) else open(done_path).read().split()
        for n in done:
            print("  done   " + n)
        for s in STEPS:
            if s.name in done:
                continue
            print("  ...    %s: %s" % (s.name, s.what), flush=True)
            if os.environ.get("FAKE_FAIL") == s.name:
                print("  FAILED %s after 3 s: see %s" % (s.name, os.path.join(work, "logs", s.name + ".log")))
                print("Run it again to carry on from this step.")
                sys.exit(1)
            done.append(s.name)
            open(done_path, "w").write("\\n".join(done) + "\\n")
            print("  ok     %s (1 s)" % s.name, flush=True)
        print("\\nAll done (0 min).")
''')


class Widget:
    def __init__(self, *args, **kw):
        self.opts = dict(kw)
        self.shown = True

    def __setitem__(self, k, v):
        self.opts[k] = v

    def __getitem__(self, k):
        return self.opts.get(k)

    def grid(self, **kw):
        self.shown = True

    def grid_remove(self):
        self.shown = False


class FakeTk(Widget):
    def __init__(self):
        super().__init__()
        self.afters = []
        self.destroyed = False

    def title(self, t):
        self.opts["title"] = t

    def resizable(self, *a):
        pass

    def iconbitmap(self, *a):
        pass

    def protocol(self, *a):
        pass

    def after(self, ms, fn):
        self.afters.append(fn)

    def destroy(self):
        self.destroyed = True


class FakeTkModule:
    Tk = FakeTk


class FakeTtk:
    Frame = Label = Button = Progressbar = Widget


class FakeDialogs:
    def __init__(self):
        self.folder = ""
        self.yes = True
        self.errors = []

    def askdirectory(self, **kw):
        return self.folder

    def askyesno(self, *a):
        return self.yes

    def showerror(self, *a):
        self.errors.append(a)


class BuilderTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        app = self.tmp.name
        self.p = builder.Paths(app)
        os.makedirs(os.path.join(app, "tools", "installer"))
        with open(os.path.join(app, "tools", "extract_game.py"), "w") as f:
            f.write(FAKE_EXTRACT)
        builder.write_text(self.p.pipeline, "tools-2")
        self.game = os.path.join(app, "game")
        os.makedirs(os.path.join(self.game, "TransGame", "CookedPC"))
        os.environ["FAKE_GAME"] = self.game
        os.environ.pop("FAKE_FAIL", None)
        self.launched = []
        self.saved = (builder.ensure_ffmpeg, builder.launch_game, builder.NEEDED_BYTES)
        builder.NEEDED_BYTES = 0
        builder.ensure_ffmpeg = lambda p, progress, stop, log: app
        builder.launch_game = lambda p: self.launched.append(p.exe)
        sys.modules.pop("extract_game", None)

    def tearDown(self):
        builder.ensure_ffmpeg, builder.launch_game, builder.NEEDED_BYTES = self.saved
        sys.modules.pop("extract_game", None)
        os.environ.pop("FAKE_GAME", None)
        os.environ.pop("FAKE_FAIL", None)
        self.tmp.cleanup()

    def make(self, dialogs=None, **kw):
        # (One window at a time, as on the player's PC: the one before is closed.)
        if getattr(self, "app", None) and self.app.lock.f:
            self.app.lock.release()
        self.dialogs = dialogs or FakeDialogs()
        self.app = builder.App(FakeTkModule, FakeTtk, self.dialogs, self.dialogs, self.p, **kw)
        return self.app

    def run_until(self, app, phases, timeout=30):
        t = time.time()
        while app.phase not in phases:
            app.pump()
            if time.time() - t > timeout:
                self.fail(f"stuck in {app.phase}")
            time.sleep(0.02)
        app.pump()

    def args(self):
        return open(os.path.join(self.p.app, "args.txt")).read().split()

    def test_a_fresh_install_builds_everything_and_marks_it_built(self):
        app = self.make(after_setup=True)
        self.assertEqual(app.phase, "idle")
        app.start()
        self.run_until(app, ("done", "failed"))
        self.assertEqual(app.phase, "done")
        self.assertEqual(builder.read_text(self.p.marker), "tools-2")
        self.assertEqual(builder.read_text(self.p.game_file), self.game)
        self.assertNotIn("--redo", self.args())
        self.assertEqual(app.bar["value"], 1000)
        self.assertEqual(app.main_button["text"], "Play")
        self.assertEqual(builder.build_state(self.p), "ready")

    def test_a_failed_step_is_carried_on_from_next_time(self):
        os.environ["FAKE_FAIL"] = "menu"
        app = self.make()
        app.start()
        self.run_until(app, ("done", "failed"))
        self.assertEqual(app.phase, "failed")
        self.assertIn('"The menus"', app.status["text"])
        self.assertTrue(app.failed_log.endswith("menu.log"))
        self.assertFalse(os.path.exists(self.p.marker))
        self.assertEqual(builder.build_state(self.p), "resume")
        # Try again, the step now working: it carries on (the Downfall map not again).
        os.environ.pop("FAKE_FAIL")
        app.start()
        self.run_until(app, ("done",))
        self.assertNotIn("--redo", self.args())
        self.assertEqual(app.progress.done, {"map_downfall", "menu", "sounds"})
        self.assertEqual(builder.read_text(self.p.marker), "tools-2")

    def test_an_update_with_other_tools_builds_everything_again(self):
        app = self.make()
        app.start()
        self.run_until(app, ("done",))
        builder.write_text(self.p.pipeline, "tools-3")
        self.assertEqual(builder.build_state(self.p), "redo")
        app = self.make(play=True)
        app.start()
        self.run_until(app, ("done",))
        self.assertIn("--redo", self.args())
        self.assertEqual(builder.read_text(self.p.marker), "tools-3")
        # (--play: the game starts once it's built.)
        [fn() for fn in app.root.afters if getattr(fn, "__name__", "") == "play_now"]
        self.assertEqual(self.launched, [self.p.exe])

    def test_built_files_play_at_once(self):
        builder.write_text(self.p.marker, "tools-2")
        app = self.make(play=True)
        self.assertEqual(self.launched, [self.p.exe])
        app = self.make()
        self.assertEqual(app.phase, "ready")
        self.assertEqual(app.second_button["text"], "Build them again")

    def test_a_game_not_found_is_asked_for(self):
        os.environ.pop("FAKE_GAME")
        dialogs = FakeDialogs()
        app = self.make(dialogs)
        self.assertIsNone(app.game)
        self.assertEqual(app.main_button["text"], "Choose its folder...")
        # A folder without the game: said so.
        dialogs.folder = self.p.app
        app.choose_game()
        self.assertEqual(len(dialogs.errors), 1)
        # Its CookedPC folder: the game's folder is found from it.
        dialogs.folder = os.path.join(self.game, "TransGame", "CookedPC")
        app.choose_game()
        self.assertEqual(app.game, self.game)
        self.run_until(app, ("done",))
        self.assertEqual(builder.read_text(self.p.game_file), self.game)

    def test_the_output_is_read_as_the_build_prints_it(self):
        self.assertEqual(builder.parse_line("  done   map_downfall\n"), ("done", "map_downfall"))
        self.assertEqual(builder.parse_line("  ...    menu: the menus (pictures)\n"), ("start", "menu"))
        self.assertEqual(builder.parse_line("  ok     menu (1203 s)\n"), ("ok", "menu"))
        self.assertEqual(builder.parse_line("  FAILED menu after 3 s: see C:\\w\\logs\\menu.log"),
                         ("failed", "menu", "C:\\w\\logs\\menu.log"))
        self.assertEqual(builder.parse_line("All done (77 min)."), ("finished",))
        self.assertIsNone(builder.parse_line("Game:   C:\\..."))
        self.assertEqual(builder.describe("Jazz (the robot and his car), the guns, the stations' props"),
                         "Jazz, the guns, the stations' props")
        self.assertEqual(builder.describe("every sound (last: it reads what the others wrote)"), "Every sound")

    def test_time_left_follows_this_pcs_pace(self):
        pr = builder.Progress([("a", ""), ("menu", ""), ("b", "")])
        self.assertEqual(pr.total, 22)
        pr.event(("done", "a"), 0)
        pr.event(("start", "menu"), 0)
        self.assertIsNone(pr.minutes_left(30))
        self.assertEqual(pr.step_number(), (2, 3))
        pr.event(("ok", "menu"), 600)
        # 20 of the weight in 10 minutes this run: the last 1 in half a minute.
        self.assertEqual(pr.minutes_left(600), 1)
        self.assertAlmostEqual(pr.fraction(), 21 / 22)
        self.assertEqual(builder.time_left_text(40), "About 40 minutes left")
        self.assertEqual(builder.time_left_text(62), "About an hour left")

    def test_one_builder_at_a_time(self):
        a = builder.Lock(self.p.lock)
        self.assertTrue(a.acquire())
        self.assertFalse(builder.Lock(self.p.lock).acquire())
        a.release()
        self.assertTrue(builder.Lock(self.p.lock).acquire())

    def test_ffmpeg_is_taken_out_of_its_download(self):
        import zipfile
        z = os.path.join(self.p.app, "ffmpeg.zip")
        with zipfile.ZipFile(z, "w") as f:
            f.writestr("ffmpeg-9.0.2-essentials_build/README.txt", "readme")
            f.writestr("ffmpeg-9.0.2-essentials_build/bin/ffprobe.exe", "probe")
            f.writestr("ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe", "the program")
        dest = os.path.join(self.p.ffmpeg_dir, "ffmpeg.exe")
        builder.extract_ffmpeg(z, dest)
        self.assertEqual(open(dest).read(), "the program")
        with zipfile.ZipFile(z, "w") as f:
            f.writestr("other/README.txt", "readme")
        with self.assertRaises(ValueError):
            builder.extract_ffmpeg(z, dest)


if __name__ == "__main__":
    unittest.main()
