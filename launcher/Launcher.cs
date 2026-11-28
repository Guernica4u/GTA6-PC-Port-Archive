// Minimal game launcher: pick the game's .exe once, pick a runtime, press PLAY. Side tabs: Launcher, Conversion
// (the runtime), Settings. Slideshow of the pictures in backgrounds\ (drop more on the window), music from the game's own audio.
// Runtimes: the archive top 3 in the site's order (AnyXboxX left out) plus ztrx, Hyphen (hyphenn, was victory-fork), cool_fox, VIctory
// and xcl, each installed in runtimes\<dir>\target\release: built there from source, or one of the site's ready-made builds
// (cool_fox's download picker) unpacked there.
// Build: build.bat (uses the C# compiler that ships with Windows, C# 5).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.IO;
using System.Linq;
using System.Net;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Windows.Forms;
using System.Xml;

[assembly: AssemblyTitle("GTA VI Launcher")]
[assembly: AssemblyDescription("Runs your game dump through the PC port runtimes")]
[assembly: AssemblyProduct("GTA VI Launcher")]
[assembly: AssemblyVersion("1.11.0.0")]
[assembly: AssemblyFileVersion("1.11.0.0")]

// One runtime project, as the archive's site describes it.
class Runtime
{
    public string Key, Name, Branch, Note, Gpu; // Gpu: "nvidia" or "amd", the cards it's made for
    public int Reached, Top, Rank; // Top: the archive's pinned place (0 = none); Rank: 1-3 in the launcher's top 3, else 0
    public bool Gone; // removed from GitHub (not just gone private); the archive keeps its last copy

    // The site's ready-made builds for this project, {label, ZIP url}, newest first, like its download picker; building from
    // source is the last choice, unless the entry has no source (SourceToo false). Choice indexes Options; Options.Count = source.
    public readonly List<string[]> Options = new List<string[]>();
    public bool SourceToo = true;
    public int Choice;
    public int Choices { get { return Options.Count + (SourceToo ? 1 : 0); } }
    public string Release { get { return Choice < Options.Count ? Options[Choice][1] : null; } }
    public string ChoiceLabel { get { return Release == null ? "Source" : Options[Choice][0].Split(new[] { " (" }, StringSplitOptions.None)[0]; } }

    // Ready-made builds the launcher may install, by ZIP name: the runner exe and its SHA-256 as the archive's release notes
    // list it, then the script that starts it and its argument ("game" = the game folder). A download whose exe doesn't match
    // is deleted, not run. Builds not listed here can't be installed.
    static readonly Dictionary<string, string[]> Builds = new Dictionary<string, string[]>
    {
        { "victory-cool_fox-willy.zip", new[] { "victory.exe", "FD83D60BA725FFBD4B67D88AF5F2E64E697663CC4152DE373A81F6CBDDA8E66D", "Play-cool_fox.cmd", "game" } },
        { "cool_fox-pareto-residency-windows-x64.zip", new[] { "victory-3d-rtx-pareto-residency.exe", "DF748C624082173ABD3E36540D7E89AB6673FDAB34D781341692960A45C2ED5E", "Play-VIctory.cmd", "pareto-residency" } },
    };
    string[] Known { get { string[] b; return Release != null && Builds.TryGetValue(Release.Substring(Release.LastIndexOf('/') + 1), out b) ? b : null; } }

    // Every runtime installs next to the launcher, in runtimes\<key> (a ready-made build in runtimes\<key>-<its release tag>),
    // into its target\release: built there, or the build unpacked there.
    public string Dir
    {
        get
        {
            if (Release == null) return Path.Combine(Launcher.Here, "runtimes", Key);
            var parts = Release.Split('/');
            return Path.Combine(Launcher.Here, "runtimes", Key + "-" + parts[parts.Length - 2]);
        }
    }

    // shortcut: a runtime without a recipe here is assumed to build like VIctory (cargo -p victory); add one when the archive's top 3 gains one that doesn't.
    public string Exe
    {
        get
        {
            var b = Known;
            return Path.Combine(Dir, @"target\release\" + (b != null ? b[0] : Key == "xcl" ? "xcl.exe" : Key == "kelvaroq" ? "kelvaroq.exe" : "victory.exe"));
        }
    }
    public bool Installed { get { return File.Exists(Exe); } }
    public bool Ready { get { return Release != null; } } // installs by download, no git or Rust

    // The console command that installs it; null when the launcher can't.
    public string Install
    {
        get
        {
            if (Release == null)
            {
                string clone = Directory.Exists(Dir) ? "" :
                    "git clone --depth 1 --single-branch --branch " + Branch + "/main " + Launcher.Archive + " \"" + Dir + "\" && ";
                return clone + "cd /d \"" + Dir + "\" && " + Build;
            }
            var b = Known;
            if (b == null) return null;
            // Download, unpack into target\release (the files may sit in a subfolder of the ZIP), then check the exe's hash.
            // shortcut: paths go in single quotes, so a launcher folder with an apostrophe in its name breaks this; quote properly if that turns up.
            string to = Path.Combine(Dir, @"target\release");
            return "powershell -NoProfile -ExecutionPolicy Bypass -Command \"$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; " +
                "[Net.ServicePointManager]::SecurityProtocol='Tls12'; $d='" + to + "'; New-Item -ItemType Directory -Force $d | Out-Null; " +
                "$z=Join-Path $d 'download.zip'; Write-Host 'Downloading " + Name + " (" + ChoiceLabel + ")...'; Invoke-WebRequest -UseBasicParsing '" + Release + "' -OutFile $z; " +
                "Expand-Archive $z $d -Force; Remove-Item $z; " +
                "$x=Get-ChildItem $d -Recurse -Filter '" + b[0] + "' | Select-Object -First 1; " +
                "if ($x -and $x.DirectoryName -ne $d) { Get-ChildItem $x.DirectoryName | Move-Item -Destination $d -Force; Remove-Item $x.DirectoryName -Recurse -Force }; " +
                "$f=Join-Path $d '" + b[0] + "'; if (-not (Test-Path $f) -or (Get-FileHash $f -Algorithm SHA256).Hash -ne '" + b[1] + "') " +
                "{ Remove-Item -Recurse -Force '" + Dir + "'; throw 'The download is not the archived build (missing, quarantined by antivirus, or a different checksum). Nothing was kept.' }; " +
                "Write-Host 'Installed.'\"";
        }
    }

    // Pulls the archive's newest copy of its branch into its folder and rebuilds it there.
    public string Update { get { return "cd /d \"" + Dir + "\" && git fetch --depth 1 origin " + Branch + "/main && git reset -q --hard FETCH_HEAD && " + Build; } }

    public string Build
    {
        get
        {
            if (Key == "xcl") return "cargo build --release -p xcl";
            if (Key == "dexnom") // its BCPack tables are fetched and hash-checked first
                return "powershell -NoProfile -ExecutionPolicy Bypass -File Initialize-FormatTables.ps1 && cargo build --locked --release -p victory --features diagnostics --bins";
            if (Key == "victory-7900xtx") return "cargo build --locked --release -p victory --features diagnostics"; // its play.ps1 -Build release
            if (Key == "cool-fox" || Key == "victory-nvidia") return "cargo build --release --locked -p victory";
            // Its CLI, then its native engine (fetches its pinned VIctory, patches it, puts kelvaroq-engine.exe in target\release\native).
            if (Key == "kelvaroq") return "cargo build --release --locked && python scripts\\build_native.py";
            return "cargo build --release -p victory";
        }
    }

    public string Run(string game)
    {
        // A ready-made build starts through the script it ships with.
        var b = Known;
        if (b != null)
            return "\"" + Path.Combine(Path.GetDirectoryName(Exe), b[2]) + "\" " + (b[3] == "game" ? "\"" + Path.GetDirectoryName(game) + "\"" : b[3]);
        string exe = "\"" + Exe + "\" ";
        if (Key == "xcl") return exe + "run \"" + Path.GetDirectoryName(game) + "\"";
        // victory-play.exe's flags; not victory-play itself, which forces saves into target\release\state inside the repo and victory refuses that.
        if (Key == "dexnom") return exe + "run \"" + game + "\" --execute --console series-x --unimplemented trap";
        // Its play.ps1 sets the tuned VICTORY_* switches and keeps our paths; exit with victory's code so a failure stays readable.
        if (Key == "victory-7900xtx")
            return "powershell -NoProfile -ExecutionPolicy Bypass -Command \"& '" + Path.Combine(Dir, @"scripts\play.ps1") + "' -Exe '" + game + "' -Build release; exit $LASTEXITCODE\"";
        // Its launcher sets the RTX-tested Series S settings and runs target\release\victory.exe on VICTORY_GAME_DIR's game_scarlett_master.exe.
        if (Key == "cool-fox") return "\"" + Path.Combine(Dir, "Play-VIctory.cmd") + "\" pareto-residency";
        if (Key == "kelvaroq") return exe + "play \"" + game + "\"";
        // Its play-nvidia.ps1: Series X, 1440p, DLSS Quality (when Get-NVIDIA-DLSS.cmd was run, else the game's own upscaling).
        if (Key == "victory-nvidia")
            return "powershell -NoProfile -ExecutionPolicy Bypass -Command \"& '" + Path.Combine(Dir, @"scripts\play-nvidia.ps1") + "' -Exe '" + game + "'; exit $LASTEXITCODE\"";
        return exe + "run \"" + game + "\" --execute";
    }
}

static class Launcher
{
    public static readonly string Here = AppDomain.CurrentDomain.BaseDirectory;
    public static readonly string Data = Path.Combine(Here, "victory-data");
    public static readonly string Settings = Path.Combine(Here, "launcher.txt");
    public const string Archive = "https://github.com/Guernica4u/GTA6-PC-Port-Archive.git";
    const string ArchiveSite = "https://raw.githubusercontent.com/Guernica4u/GTA6-PC-Port-Archive/main/docs/index.html";
    static readonly string[] Wanted = { "ztrx", "hyphenn", "victory-fork", "victory", "xcl", "cool-fox", "kelvaroq", "victory-nvidia", "belobug", "victory-7900xtx" }; // the user's picks: Zettrox, Hyphen (renamed), VIctory, xcl, cool_fox, Kelvaroq, VIctory-NVIDIA, Belobug, 7900XTX; kept even once gone or unpinned
    public const string Repo = "https://github.com/Guernica4u/GTA6-PC-Port-Archive"; // the site's ${REPO}
    static readonly string[] Excluded = { "anyxboxx" }; // the user doesn't want it, even when it ranks
    static readonly Dictionary<string, string> Names = new Dictionary<string, string>
    {
        { "hyphenn", "Hyphen" }, { "victory-fork", "Hyphen" }, { "dexnom", "DexNom" }, { "belobug", "Belobug" }, { "victory-7900xtx", "7900XTX" },
    }; // cool_fox :CGBJ and NEWEST CGBJ private keep the site's names, as the user asked
    // The cards each is made for, where the site doesn't say: from their READMEs (ztrx: NVIDIA RTX only; VIctory tested on an RX 6700 XT; xcl developed on RDNA4).
    static readonly Dictionary<string, string> Gpus = new Dictionary<string, string> { { "ztrx", "nvidia" }, { "victory", "amd" }, { "xcl", "amd" } };

    public static XmlDocument Manifest(string dir)
    {
        string cfg = Path.Combine(dir, "MicrosoftGame.config");
        if (!File.Exists(cfg)) return null;
        var doc = new XmlDocument();
        doc.Load(cfg);
        return doc;
    }

    // The title exe: what MicrosoftGame.config names, else the largest .exe in the folder.
    public static string FindTitleExe(string dir)
    {
        var doc = Manifest(dir);
        var node = doc == null ? null : doc.SelectSingleNode("//*[local-name()='Executable'][@Name]");
        if (node != null)
        {
            string exe = Path.Combine(dir, node.Attributes["Name"].Value);
            if (File.Exists(exe)) return exe;
        }
        return new DirectoryInfo(dir).GetFiles("*.exe").OrderByDescending(f => f.Length).Select(f => f.FullName).FirstOrDefault();
    }

    // The archive site's PROJECTS table: key, name, branch, reached, top, note, gone.
    public static List<Runtime> ParseArchive(string html)
    {
        // An entry opens with its repo, or (a ready-made build kept as a release, no repo) with its saved date.
        var starts = Regex.Matches(html, "^\\s*\"?([\\w-]+)\"?: \\{ (?:repo|saved): \"", RegexOptions.Multiline);
        var list = new List<Runtime>();
        for (int i = 0; i < starts.Count; i++)
        {
            int end = i + 1 < starts.Count ? starts[i + 1].Index : Math.Min(html.Length, starts[i].Index + 4000);
            string chunk = html.Substring(starts[i].Index, end - starts[i].Index);
            Func<string, string> str = k => { var m = Regex.Match(chunk, "\\b" + k + ": \"((?:[^\"\\\\]|\\\\.)*)\""); return m.Success ? m.Groups[1].Value : null; };
            var reached = Regex.Match(chunk, "\\breached: *(\\d+)");
            var top = Regex.Match(chunk, "\\btop: *(\\d+)");
            string key = starts[i].Groups[1].Value;
            var rt = new Runtime
            {
                Key = key, Name = str("name") ?? key, Branch = str("branch") ?? key, Note = str("note") ?? "", Gpu = str("gpu"),
                Reached = reached.Success ? int.Parse(reached.Groups[1].Value) : 0,
                Top = top.Success ? int.Parse(top.Groups[1].Value) : 0,
                // As the site: a repo gone private is still alive (work goes on there).
                Gone = Regex.IsMatch(chunk, "\\bgone: *(true|\")") && !Regex.IsMatch(chunk, "\\bprivate: *true"),
            };
            // Its download picker's builds ({ label, url } with ${REPO}); an entry without a repo is one ready-made build.
            foreach (Match b in Regex.Matches(chunk, "\\{ *label: *\"((?:[^\"\\\\]|\\\\.)*)\", *url: *[`\"]([^`\"]+)[`\"] *\\}"))
                rt.Options.Add(new[] { b.Groups[1].Value, b.Groups[2].Value.Replace("${REPO}", Repo) });
            if (!Regex.IsMatch(chunk, "\\brepo: \"") && str("release") != null)
            {
                rt.Options.Clear();
                rt.Options.Add(new[] { rt.Name, str("release").Replace("${REPO}", Repo) });
                rt.SourceToo = false;
            }
            list.Add(rt);
        }
        return list;
    }

    // Top 3 exactly as the site ranks its live projects (pinned places, then how far each gets, then name), plus the
    // user's picks, which stay listed even once gone: the archive still has their last copy.
    public static List<Runtime> Choose(List<Runtime> all)
    {
        var ranked = all.Where(r => !Excluded.Contains(r.Key) && (!r.Gone || Wanted.Contains(r.Key)))
            .OrderBy(r => r.Top > 0 ? r.Top : int.MaxValue).ThenByDescending(r => r.Reached).ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase).ToList();
        var top = ranked.Where(r => !r.Gone).Take(3).ToList();
        var pick = top.Concat(ranked.Where(r => Wanted.Contains(r.Key) && !top.Contains(r))).ToList();
        for (int i = 0; i < pick.Count; i++) pick[i].Rank = i < 3 ? i + 1 : 0;
        foreach (var r in pick) { string n; if (Names.TryGetValue(r.Key, out n)) r.Name = n; if (r.Gpu == null && Gpus.TryGetValue(r.Key, out n)) r.Gpu = n; }
        return pick;
    }

    // Offline fallback: the archive on 28 Nov 2026: cool_fox :CGBJ (private, with two ready-made builds) first, VIctory-NVIDIA second,
    // Kelvaroq third; Belobug and 7900XTX were taken off GitHub.
    const string Builtin =
        "  \"cool-fox\": { repo: \"codec-d/cool_fox\", gone: \"26 Nov 2026\", private: true, name: \"cool_fox :CGBJ\", gpu: \"amd\", top: 1, branch: \"cool-fox\", reached: 5,\n" +
        "    builds: [\n" +
        "      { label: \"willy build 30e54b4 (newest)\", url: `${REPO}/releases/download/cgbj-willy-30e54b4/victory-cool_fox-willy.zip` },\n" +
        "      { label: \"pareto-residency build (Defender flags it)\", url: `${REPO}/releases/download/cgbj-pareto-residency/cool_fox-pareto-residency-windows-x64.zip` },\n" +
        "    ],\n" +
        "    note: \"Newest build: about 30 fps median on an RTX 4090, 60 fps cap, keyboard and mouse, memory peak about 27 GB.\" },\n" +
        "  \"victory-7900xtx\": { repo: \"avatarmanz/VIctory-7900XTX\", gone: \"27 Nov 2026\", name: \"VIctory-7900XTX\", gpu: \"amd\", branch: \"victory-7900xtx\", reached: 5, note: \"Prologue and first mission at 13-22 fps. Needs about 64 GB of RAM.\" },\n" +
        "  ztrx: { repo: \"Zettrox/ztrx\", gone: \"25 Nov 2026\", name: \"ztrx\", branch: \"ztrx\", reached: 5, note: \"The prologue is playable. NVIDIA only.\" },\n" +
        "  hyphenn: { repo: \"fxceless/victory-fork\", name: \"hyphenn\", gpu: \"nvidia\", top: 5, branch: \"hyphenn\", reached: 4, note: \"First gameplay scene at 2-4 fps.\" },\n" +
        "  dexnom: { repo: \"DexNom/VIctory\", name: \"DexNom Runtime\", gpu: \"amd\", top: 4, branch: \"dexnom\", reached: 4, note: \"VIctory fork tuned on Radeon, experimental ray tracing.\" },\n" +
        "  belobug: { repo: \"Belobug/VIctory-Belobug-AMD\", gone: \"27 Nov 2026\", name: \"VIctory-Belobug-AMD\", gpu: \"amd\", branch: \"belobug\", reached: 4, note: \"The world renders lit at 5-7 fps on an RX 9070 XT.\" },\n" +
        "  kelvaroq: { repo: \"stellarr-studio/kelvaroq\", name: \"Kelvaroq\", gpu: \"nvidia\", top: 3, branch: \"kelvaroq\", reached: 5, note: \"Plays the prologue at about 10 fps at 4K on an RTX 5080.\" },\n" +
        "  \"victory-nvidia\": { repo: \"trustno1-coder/VIctory-NVIDIA\", name: \"VIctory-NVIDIA\", gpu: \"nvidia\", top: 2, branch: \"victory-nvidia\", reached: 5, note: \"Opening rooms at 26-30 fps on an RTX 5070 with DLSS. Needs 32 GB of RAM.\" },\n" +
        "  victory: { repo: \"Marvisak/VIctory\", name: \"VIctory\", branch: \"victory\", reached: 3, note: \"Menus and the world intro.\" },\n" +
        "  xcl: { repo: \"ivall/xcl\", name: \"xcl\", branch: \"xcl\", reached: 2, note: \"Its own code, not VIctory. Menus at 30 fps; gameplay not reached yet.\" },\n";

    public static string CachedArchive { get { return Path.Combine(Data, "archive.html"); } }

    public static List<Runtime> LoadRuntimes(bool online)
    {
        string html = null;
        if (online)
        {
            try
            {
                ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072; // TLS 1.2
                using (var wc = new WebClient { Encoding = Encoding.UTF8 }) html = wc.DownloadString(ArchiveSite + "?t=" + DateTime.UtcNow.Ticks); // past raw.githubusercontent's 5-minute cache
                if (ParseArchive(html).Count == 0) html = null;
                else { Directory.CreateDirectory(Data); File.WriteAllText(CachedArchive, html); }
            }
            catch { html = null; }
        }
        if (html == null && File.Exists(CachedArchive)) html = File.ReadAllText(CachedArchive);
        var all = html == null ? new List<Runtime>() : ParseArchive(html);
        return Choose(all.Count > 0 ? all : ParseArchive(Builtin));
    }

    public static readonly Version Version = new Version(Assembly.GetExecutingAssembly().GetName().Version.ToString(3));

    // The newest launcher-vX tag in GitHub's releases list.
    public static Version NewestLauncher(string json)
    {
        Version best = null;
        foreach (Match m in Regex.Matches(json, "\"tag_name\": *\"launcher-v(\\d+\\.\\d+\\.\\d+)\""))
        {
            var v = new Version(m.Groups[1].Value);
            if (best == null || v > best) best = v;
        }
        return best;
    }

    public static Version CheckLauncher()
    {
        ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072; // TLS 1.2
        using (var wc = new WebClient { Encoding = Encoding.UTF8 })
        {
            wc.Headers["User-Agent"] = "GTA-VI-Launcher"; // GitHub's API refuses requests without one
            return NewestLauncher(wc.DownloadString("https://api.github.com/repos/Guernica4u/GTA6-PC-Port-Archive/releases?per_page=100"));
        }
    }

    // git with no window; its exit code and output.
    static int Git(string args, string dir, out string output)
    {
        var psi = new ProcessStartInfo("git", args) { WorkingDirectory = dir, UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true };
        using (var p = Process.Start(psi)) { output = p.StandardOutput.ReadToEnd(); p.WaitForExit(); return p.ExitCode; }
    }

    // Folders of installed source runtimes whose archive branch has new code since they were built.
    // README, docs, images and GitHub workflow changes don't count: those come every few hours and need no rebuild.
    public static List<string> Outdated(IEnumerable<Runtime> list)
    {
        var res = new List<string>();
        string heads, o;
        if (Git("ls-remote " + Archive + " \"refs/heads/*/main\"", Here, out heads) != 0) return res;
        foreach (var rt in list.Where(r => r.Release == null && r.Installed && Directory.Exists(Path.Combine(r.Dir, ".git"))))
        {
            var m = Regex.Match(heads, "^([0-9a-f]{40})\\trefs/heads/" + Regex.Escape(rt.Branch) + "/main\\r?$", RegexOptions.Multiline);
            if (!m.Success || Git("rev-parse HEAD", rt.Dir, out o) != 0 || o.Trim() == m.Groups[1].Value) continue;
            if (Git("fetch -q --depth 1 origin " + rt.Branch + "/main", rt.Dir, out o) == 0 &&
                Git("diff --quiet HEAD FETCH_HEAD -- . \":(exclude)*.md\" \":(exclude)docs\" \":(exclude).github\" \":(exclude)*.svg\" \":(exclude)*.png\" \":(exclude)*.jpg\" \":(exclude)*.gif\"", rt.Dir, out o) == 1)
                res.Add(rt.Dir);
        }
        return res;
    }

    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();

    [STAThread]
    static int Main(string[] args)
    {
        if (args.Length > 0 && args[0] == "--selftest") return SelfTest();
        SetProcessDPIAware();
        Application.EnableVisualStyles();
        string[] saved = File.Exists(Settings) ? File.ReadAllLines(Settings) : new string[0];
        string exe = saved.Length > 0 ? saved[0].Trim() : "";
        try { if (Directory.Exists(exe)) exe = FindTitleExe(exe); } catch { } // settings from v1.0 held the folder
        int song;
        if (saved.Length < 4 || !int.TryParse(saved[3], out song) || song < 0 || song >= GameMusic.Songs.Length) song = 0;
        Application.Run(new LauncherForm(exe != null && File.Exists(exe) ? exe : null, saved.Length > 1 && saved[1].Trim().Length > 0 ? saved[1].Trim() : null,
            saved.Length > 2 && saved[2].Trim() == "muted", song, saved.Length > 4 ? saved[4].Trim() : ""));
        return 0;
    }

    public static void Error(string msg)
    {
        MessageBox.Show(msg, "GTA VI Launcher", MessageBoxButtons.OK, MessageBoxIcon.Error);
    }

    static int SelfTest()
    {
        // Next to the exe, not %TEMP%: antivirus on this machine denied unsigned exes writing there.
        string t = Path.Combine(Here, "selftest-tmp");
        Directory.CreateDirectory(t);
        try
        {
            File.WriteAllText(Path.Combine(t, "big.exe"), new string('x', 100));
            File.WriteAllText(Path.Combine(t, "Game.exe"), "x");
            if (FindTitleExe(t) != Path.Combine(t, "big.exe")) return 1;
            File.WriteAllText(Path.Combine(t, "MicrosoftGame.config"),
                "<Game xmlns=\"http://x\"><ExecutableList><Executable Name=\"Game.exe\" Id=\"Game\"/></ExecutableList>" +
                "<ShellVisuals DefaultDisplayName=\"Test Title\" SplashScreenImage=\"s.png\"/></Game>");
            if (FindTitleExe(t) != Path.Combine(t, "Game.exe")) return 2;
            if (LauncherForm.Visual(Manifest(t), "DefaultDisplayName") != "Test Title") return 4;
            foreach (var f in Directory.GetFiles(t, "*.exe")) File.Delete(f);
            if (FindTitleExe(t) != null) return 3;
        }
        finally { Directory.Delete(t, true); }

        // Ranking: pinned places, then reached, ties by name, gone projects skipped unless picked, the user's picks always present.
        string site = Builtin.Replace("reached: 3", "reached: 3, note: \"x \\\"q\\\"\"") +
            "  anyxboxx: { repo: \"LooperSalty/AnyXboxX\", name: \"AnyXboxX\", branch: \"anyxboxx\", reached: 4 },\n" +
            "  tuxbox: { repo: \"brcly/TuxBox\", name: \"TuxBox\", branch: \"tuxbox\", reached: 2 },\n" +
            "  \"leonida-old\": { repo: \"x/L\", gone: \"23 Nov 2026\", name: \"Leonida\", reached: 9 },\n";
        var keys = string.Join(",", Choose(ParseArchive(site)).Select(r => r.Key + r.Rank).ToArray());
        if (keys != "cool-fox1,victory-nvidia2,kelvaroq3,hyphenn0,victory-7900xtx0,ztrx0,belobug0,victory0,xcl0") return 5;
        // The download picker: ready-made builds install by download with their pinned checksum and start through their
        // own script; the last choice builds from source. A release-only entry is one build, no source.
        var all = ParseArchive(Builtin);
        var fox = all.First(r => r.Key == "cool-fox");
        if (fox.Gone || fox.Choices != 3 || !fox.Install.Contains("FD83D60B") || !fox.Dir.EndsWith(@"runtimes\cool-fox-cgbj-willy-30e54b4") ||
            !fox.Run(@"C:\g\game.exe").EndsWith("Play-cool_fox.cmd\" \"C:\\g\"")) return 10;
        fox.Choice = 1;
        if (!fox.Install.Contains("DF748C62") || !fox.Run(@"C:\g\game.exe").EndsWith("Play-VIctory.cmd\" pareto-residency")) return 11;
        fox.Choice = 2;
        if (fox.Release != null || !fox.Install.Contains("cargo build") || fox.ChoiceLabel != "Source" ||
            all.First(r => r.Key == "xcl").Choices != 1) return 12;
        var one = ParseArchive("  solo: { saved: \"x\", name: \"Solo\", release: \"${REPO}/releases/download/t/cool_fox-pareto-residency-windows-x64.zip\" },\n")[0];
        if (one.Choices != 1 || one.SourceToo || !one.Install.Contains("DF748C62") || !one.Release.StartsWith(Repo + "/releases/")) return 13;
        foreach (var r in Choose(ParseArchive(Builtin))) // every runtime installs into runtimes\<dir>\target\release
            if (!r.Exe.StartsWith(Path.Combine(r.Dir, @"target\release\")) || !r.Dir.StartsWith(Path.Combine(Here, "runtimes") + @"\")) return 9;
        if (Choose(ParseArchive(Builtin)).First(r => r.Key == "hyphenn").Name != "Hyphen") return 6;
        var kel = all.First(r => r.Key == "kelvaroq");
        if (!kel.Exe.EndsWith(@"runtimes\kelvaroq\target\release\kelvaroq.exe") || !kel.Run(@"C:\g\game.exe").EndsWith(@"kelvaroq.exe"" play ""C:\g\game.exe""") ||
            !kel.Build.Contains("build_native.py") || !all.First(r => r.Key == "victory-nvidia").Run(@"C:\g\game.exe").Contains(@"scripts\play-nvidia.ps1' -Exe 'C:\g\game.exe'")) return 15;
        // Launcher updates: the newest launcher-vX tag, other releases ignored; runtime updates pull the branch and rebuild.
        var nl = NewestLauncher("[{\"tag_name\": \"cgbj-willy-30e54b4\"}, {\"tag_name\":\"launcher-v1.9.0\"}, {\"tag_name\": \"launcher-v1.10.1\"}, {\"tag_name\": \"launcher\"}]");
        if (nl == null || nl.ToString() != "1.10.1" || !(new Version("1.10.2") > Version || Version >= nl) ||
            !all.First(r => r.Key == "kelvaroq").Update.EndsWith("git fetch --depth 1 origin kelvaroq/main && git reset -q --hard FETCH_HEAD && " + all.First(r => r.Key == "kelvaroq").Build)) return 16;
        // Every runtime says which cards it's made for: the site's gpu, else the launcher's own list.
        if (string.Join(",", Choose(ParseArchive(Builtin)).Select(r => r.Key + ":" + r.Gpu).ToArray()) !=
            "cool-fox:amd,victory-nvidia:nvidia,kelvaroq:nvidia,hyphenn:nvidia,victory-7900xtx:amd,ztrx:nvidia,belobug:amd,victory:amd,xcl:amd") return 14;

        // Opus packet lengths: 20 ms CELT, 2x10 ms SILK, 3x20 ms CELT (code 3, count in byte 2).
        if (OpusDecoder.Samples(new byte[] { 0xF8 }, 0, 1) != 960 || OpusDecoder.Samples(new byte[] { 0x01 }, 0, 1) != 960 ||
            OpusDecoder.Samples(new byte[] { 0xFB, 0x03 }, 0, 2) != 2880) return 7;
        // Decoding through Windows' Opus decoder: a CELT silence packet gives 20 ms of samples.
        using (var opus = new OpusDecoder()) if (opus.Decode(new byte[] { 0xF8, 0xFF, 0xFE }, 0, 3, new float[4000], 0) != 960) return 8;
        return 0;
    }
}

// Borderless window, custom-drawn. Kept light: every slide is composed once at window size and kept as a GDI bitmap,
// blitted (crossfades with AlphaBlend); between fades only the PLAY button's glow repaints, and nothing while dragging.
class LauncherForm : Form
{
    [DllImport("dwmapi.dll")] static extern int DwmSetWindowAttribute(IntPtr hwnd, int attr, ref int value, int size);
    [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr hdc);
    [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr hdc, IntPtr obj);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr hdc);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr obj);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dst, int x, int y, int w, int h, IntPtr src, int sx, int sy, int rop);
    [DllImport("msimg32.dll")] static extern bool AlphaBlend(IntPtr dst, int x, int y, int w, int h, IntPtr src, int sx, int sy, int sw, int sh, int blend);

    string exe, title, hover, state = "", runtimeKey; // state: "", "installing", "updating", "running"
    HashSet<string> outdated = new HashSet<string>(); // runtime folders with new code in the archive
    Version newer; // a newer launcher release, once GitHub says so
    bool muted, moving, musicBusy;
    int tab, song; // tab: 0 Launcher, 1 Conversion, 2 Settings; song: index into GameMusic.Songs
    static readonly string[] Tabs = { "Launcher", "Conversion", "Settings" };
    static readonly string[] TabGlyphs = { "", "", "" }; // Segoe MDL2: Home, Switch, Settings
    List<Runtime> runtimes;
    readonly List<string> art = new List<string>(); // slideshow image files; a null entry is the launcher's own sunset
    IntPtr shown, prevBmp, nextBmp; // slides as HBITMAPs at slideSize: on screen, fading out, composed ahead
    Size slideSize;
    int slide, nextIndex, ticks; // indexes into art
    bool preparing;
    double shownAt, fadeFrom = -9;
    const double SlideSeconds = 9, FadeSeconds = 1.2;
    readonly float s; // DPI scale
    Rectangle closeR, maxR, minR, muteR, playR;
    readonly Dictionary<string, Rectangle> hits = new Dictionary<string, Rectangle>(); // clickable areas of the last full paint
    readonly Font titleF, tagF, pathF, linkF, playF, smallF, pillF, iconF, tabF;
    readonly Icon smallIcon;
    readonly Stopwatch clock = Stopwatch.StartNew();
    readonly ToolTip tip = new ToolTip();
    System.Media.SoundPlayer player;
    string musicWav;
    static readonly Color Accent = Color.FromArgb(255, 92, 141);

    int S(float v) { return (int)Math.Round(v * s); }
    int Rail { get { return S(76); } } // the side tabs' width
    Runtime Current { get { return runtimes.FirstOrDefault(r => r.Key == runtimeKey) ?? runtimes[0]; } }

    string picks; // the builds picked in the Conversion tab, "key|zip url or source" separated by spaces

    // Applies the remembered build picks to a freshly loaded runtime list; the default is the site's first (newest) build.
    List<Runtime> WithPicks(List<Runtime> list)
    {
        foreach (var rt in list)
            foreach (var p in picks.Split(' '))
                if (p.StartsWith(rt.Key + "|"))
                {
                    string want = p.Substring(rt.Key.Length + 1);
                    int i = rt.Options.FindIndex(o => o[1] == want);
                    rt.Choice = i >= 0 ? i : want == "source" && rt.SourceToo ? rt.Options.Count : 0;
                }
        return list;
    }

    public LauncherForm(string exe, string runtimeKey, bool muted, int song, string picks)
    {
        this.picks = picks;
        using (var g = CreateGraphics()) s = g.DpiX / 96f;
        this.exe = exe;
        this.muted = muted;
        this.song = song;
        Text = "GTA VI Launcher";
        FormBorderStyle = FormBorderStyle.None;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(S(960), S(540));
        BackColor = Color.Black;
        DoubleBuffered = true;
        KeyPreview = true;
        AllowDrop = true; // drop pictures on the window to add them to the slideshow
        Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
        smallIcon = new Icon(Icon, S(16), S(16));
        titleF = Fnt("Bahnschrift SemiBold Condensed", 64, "Segoe UI Semibold");
        tagF = Fnt("Bahnschrift SemiBold", 13, "Segoe UI Semibold");
        playF = Fnt("Bahnschrift SemiBold", 22, "Segoe UI Semibold");
        pathF = Fnt("Segoe UI", 13, "Segoe UI");
        linkF = new Font(pathF, FontStyle.Underline);
        smallF = Fnt("Bahnschrift SemiBold", 12, "Segoe UI Semibold");
        pillF = Fnt("Bahnschrift SemiBold", 14, "Segoe UI Semibold");
        iconF = Fnt("Segoe MDL2 Assets", 11, "Segoe UI Symbol");
        tabF = Fnt("Segoe MDL2 Assets", 20, "Segoe UI Symbol");

        // Cached ranking now, the live one from the archive as soon as it arrives.
        runtimes = WithPicks(Launcher.LoadRuntimes(false));
        this.runtimeKey = runtimeKey ?? (runtimes.FirstOrDefault(r => r.Installed) ?? runtimes[0]).Key;
        new Thread(() =>
        {
            var live = Launcher.LoadRuntimes(true);
            OnUi(() => { runtimes = WithPicks(live); Invalidate(); });
            try { var o = Launcher.Outdated(live); OnUi(() => { outdated = new HashSet<string>(o); Invalidate(); }); }
            catch { } // no git or offline: no update notices this time
        }) { IsBackground = true }.Start();
        new Thread(() =>
        {
            try { var v = Launcher.CheckLauncher(); if (v != null && v > Launcher.Version) OnUi(() => { newer = v; Invalidate(); }); }
            catch { } // offline or rate-limited
        }) { IsBackground = true }.Start();

        LayoutUi();
        LoadGame(false);
        var timer = new System.Windows.Forms.Timer { Interval = 40 };
        timer.Tick += (o, e) => Tick();
        timer.Start();
    }

    Font Fnt(string name, float px, string fallback)
    {
        var f = new Font(name, px * s, GraphicsUnit.Pixel);
        if (f.Name == name) return f;
        f.Dispose();
        return new Font(fallback, px * s, GraphicsUnit.Pixel);
    }

    protected override CreateParams CreateParams
    {
        get
        {
            var cp = base.CreateParams;
            cp.ClassStyle |= 0x20000; // CS_DROPSHADOW
            cp.Style |= 0x30000;      // WS_MINIMIZEBOX | WS_MAXIMIZEBOX: taskbar click minimizes, Win+Up and double-click go full screen
            return cp;
        }
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        int round = 2;
        DwmSetWindowAttribute(Handle, 33, ref round, 4); // Win11 rounded corners
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == 0x231) moving = true;       // WM_ENTERSIZEMOVE: no repaints while dragging
        else if (m.Msg == 0x232) moving = false; // WM_EXITSIZEMOVE
        base.WndProc(ref m);
        // Drag the window from anywhere that isn't a button.
        if (m.Msg == 0x84 && (int)m.Result == 1)
        {
            long lp = m.LParam.ToInt64();
            var p = PointToClient(new Point((short)(lp & 0xffff), (short)((lp >> 16) & 0xffff)));
            if (Hit(p) == null) m.Result = (IntPtr)2; // HTCAPTION
        }
    }

    void Tick()
    {
        if (moving || WindowState == FormWindowState.Minimized) return;
        double t = clock.Elapsed.TotalSeconds;
        if (nextBmp != IntPtr.Zero && t - shownAt > SlideSeconds)
        {
            if (prevBmp != IntPtr.Zero) DeleteObject(prevBmp);
            prevBmp = shown;
            shown = nextBmp;
            nextBmp = IntPtr.Zero;
            slide = nextIndex;
            shownAt = fadeFrom = t;
            PrepareNext();
        }
        else if (prevBmp != IntPtr.Zero && t - fadeFrom > FadeSeconds + 0.5) { DeleteObject(prevBmp); prevBmp = IntPtr.Zero; }
        if (t < 0.7 || t - fadeFrom < FadeSeconds + 0.1) Invalidate();
        else if (tab == 0 && state == "" && ++ticks % 2 == 0) Invalidate(Glow); // the glow only, at 12 fps
    }

    protected override void OnResize(EventArgs e)
    {
        base.OnResize(e);
        if (WindowState == FormWindowState.Minimized || shown == IntPtr.Zero || slideSize == ClientSize) return;
        LayoutUi();
        ShowSlide(slide);
    }

    void ToggleFullScreen()
    {
        WindowState = WindowState == FormWindowState.Maximized ? FormWindowState.Normal : FormWindowState.Maximized;
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (e.KeyCode == Keys.F11 || (e.KeyCode == Keys.Enter && e.Alt)) ToggleFullScreen();
        else if (e.KeyCode == Keys.Escape && WindowState == FormWindowState.Maximized) WindowState = FormWindowState.Normal;
    }

    Rectangle Glow { get { return Rectangle.Inflate(playR, S(10), S(10)); } }

    void LayoutUi()
    {
        int w = ClientSize.Width, h = ClientSize.Height, b = S(46), bh = S(34);
        closeR = new Rectangle(w - b, 0, b, bh);
        maxR = new Rectangle(w - 2 * b, 0, b, bh);
        minR = new Rectangle(w - 3 * b, 0, b, bh);
        muteR = new Rectangle(w - 4 * b, 0, b, bh);
        playR = new Rectangle(w - S(256), h - S(100), S(216), S(56));
    }

    string Hit(Point p)
    {
        if (closeR.Contains(p)) return "close";
        if (maxR.Contains(p)) return "max";
        if (minR.Contains(p)) return "min";
        if (musicWav != null && muteR.Contains(p)) return "mute";
        if (tab == 0 && playR.Contains(p)) return "play";
        foreach (var kv in hits) if (kv.Value.Contains(p)) return kv.Key;
        return null;
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        string h = Hit(e.Location);
        if (h == hover) return;
        hover = h;
        Cursor = h == null ? Cursors.Default : Cursors.Hand;
        tip.Hide(this);
        if (h != null && h.StartsWith("row") && runtimes[int.Parse(h.Substring(3))].Note.Length > 0) // the row cuts long notes short
            tip.Show(runtimes[int.Parse(h.Substring(3))].Note, this, hits[h].X + S(60), hits[h].Bottom + S(2), 8000);
        else if (h != null && h.StartsWith("opt"))
        {
            var rt = runtimes[int.Parse(h.Substring(3))];
            tip.Show((rt.Release == null ? "Build from source" : rt.Options[rt.Choice][0]) + ". Click for the next build.", this, hits[h].X, hits[h].Bottom + S(2), 6000);
        }
        else if (h == "mute") tip.Show(muted ? "Play the game's music" : "Mute music", this, muteR.X - S(40), muteR.Bottom + S(4), 3000);
        Invalidate();
    }

    protected override void OnMouseLeave(EventArgs e) { hover = null; tip.Hide(this); Invalidate(); }

    protected override void OnMouseClick(MouseEventArgs e)
    {
        try
        {
            string h = Hit(e.Location);
            if (h == "close") Close();
            else if (h == "min") WindowState = FormWindowState.Minimized;
            else if (h == "max") ToggleFullScreen();
            else if (h == "mute") { muted = !muted; Save(); PlayMusic(); Invalidate(); }
            else if (h == "change") Pick();
            else if (h == "app-update") UpdateLauncher();
            else if (h == "play") { if (state == "" && (exe != null || Pick())) Play(); }
            else if (h == "song-" || h == "song+")
            {
                song = (song + (h == "song+" ? 1 : GameMusic.Songs.Length - 1)) % GameMusic.Songs.Length;
                Save();
                LoadMusic(false);
                Invalidate();
            }
            else if (h == "folder") { Directory.CreateDirectory(Backgrounds); Process.Start("explorer.exe", "\"" + Backgrounds + "\""); }
            else if (h != null && h.StartsWith("tab")) { tab = int.Parse(h.Substring(3)); hover = null; Invalidate(); }
            else if (h != null && h.StartsWith("opt") && state == "")
            {
                var rt = runtimes[int.Parse(h.Substring(3))];
                rt.Choice = (rt.Choice + 1) % rt.Choices;
                picks = string.Join(" ", picks.Split(' ').Where(p => p.Length > 0 && !p.StartsWith(rt.Key + "|"))
                    .Concat(new[] { rt.Key + "|" + (rt.Release ?? "source") }).ToArray());
                runtimeKey = rt.Key;
                Save();
                Invalidate();
            }
            else if (h != null && h.StartsWith("row") && state == "")
            {
                int i = int.Parse(h.Substring(3));
                if (i < runtimes.Count) { runtimeKey = runtimes[i].Key; Save(); Invalidate(); }
            }
        }
        catch (Exception ex) { Launcher.Error(ex.Message); }
    }

    void Save() { File.WriteAllLines(Launcher.Settings, new[] { exe ?? "", runtimeKey, muted ? "muted" : "", song.ToString(), picks }); }

    bool Pick()
    {
        using (var d = new OpenFileDialog
        {
            Title = "Select your game's .exe",
            Filter = "Game (*.exe, MicrosoftGame.config)|*.exe;MicrosoftGame.config",
            InitialDirectory = exe != null ? Path.GetDirectoryName(exe) : "",
        })
        {
            if (d.ShowDialog(this) != DialogResult.OK) return false;
            string chosen = d.FileName.EndsWith(".config", StringComparison.OrdinalIgnoreCase)
                ? Launcher.FindTitleExe(Path.GetDirectoryName(d.FileName)) : d.FileName;
            if (chosen == null) { Launcher.Error("No game executable next to that file."); return false; }
            exe = chosen;
            Save();
            LoadGame(true);
            return true;
        }
    }

    void Play()
    {
        var rt = Current;
        if (rt.Installed && outdated.Contains(rt.Dir))
        {
            var a = MessageBox.Show(this, rt.Name + " has new code in the archive.\n\nUpdate it now? The launcher downloads the new code and rebuilds it (a few minutes; a console window shows the progress).\n\nNo plays the version you have.",
                "GTA VI Launcher", MessageBoxButtons.YesNoCancel, MessageBoxIcon.Question);
            if (a == DialogResult.No) { Start(rt); return; }
            if (a != DialogResult.Yes) return;
            var u = RunConsole(rt.Update, Launcher.Here);
            SetState("updating");
            u.Exited += (o, e) => OnUi(() =>
            {
                SetState("");
                if (u.ExitCode == 0) { outdated.Remove(rt.Dir); Invalidate(); }
                else Launcher.Error("Updating " + rt.Name + " failed. The console window showed why. The version you had is still there.");
            });
            return;
        }
        if (rt.Installed) { Start(rt); return; }

        string install = rt.Install;
        if (install == null) { Launcher.Error("The launcher can't install " + rt.Name + " yet."); return; }
        string ask = rt.Ready
            ? rt.Name + " (" + rt.ChoiceLabel + ") isn't installed yet. Download this ready-made build from the archive now?\n\n" +
              "Its exe isn't signed, and antivirus may flag or remove it. The launcher checks the download against the archive's checksum and runs nothing that doesn't match."
            : rt.Name + " isn't installed yet. Download it from the archive and build it now?\n\n" +
              "Needs git and Rust. Takes a few minutes; a console window shows the progress.";
        if (MessageBox.Show(this, ask, "GTA VI Launcher", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
        var p = RunConsole(install, Launcher.Here);
        SetState("installing");
        p.Exited += (o, e) => OnUi(() =>
        {
            SetState("");
            if (rt.Installed) Start(rt);
            else Launcher.Error("Installing " + rt.Name + " failed. The console window showed why" + (rt.Ready ? "." : " (is git and Rust installed?)."));
        });
    }

    // Opens the new release's page to download it. Not a self-replacing update: Windows Defender flagged an unsigned exe that
    // downloads and swaps in its own exe (Behavior:Win32/DefenseEvasion.A!ml) and then blocked that launcher's exe for good.
    void UpdateLauncher()
    {
        if (newer == null) return;
        if (MessageBox.Show(this, "GTA VI Launcher " + newer.ToString(3) + " is out (you have " + Launcher.Version.ToString(3) + ").\n\n" +
            "Open its download page? Unzip it over this folder; your settings, runtimes and pictures stay.",
            "GTA VI Launcher", MessageBoxButtons.YesNo, MessageBoxIcon.Information) != DialogResult.Yes) return;
        Process.Start(Launcher.Repo + "/releases/tag/launcher-v" + newer.ToString(3));
    }

    void Start(Runtime rt)
    {
        string dir = Path.GetDirectoryName(exe);
        var p = RunConsole(rt.Run(exe), dir, psi =>
        {
            psi.EnvironmentVariables["VICTORY_GAME_DIR"] = dir;
            // Kept next to the launcher: antivirus here blocked unsigned exes writing to %LOCALAPPDATA%.
            // Cache per runtime (each prepares the game its own way); saves shared.
            psi.EnvironmentVariables["VICTORY_CACHE_DIR"] = Path.Combine(Launcher.Data, Path.GetFileName(rt.Dir), "cache"); // one per build
            psi.EnvironmentVariables["VICTORY_DATA_DIR"] = Path.Combine(Launcher.Data, "data");
            if (rt.Key == "dexnom") psi.EnvironmentVariables["VICTORY_DEBUG_HUD"] = "1"; // as victory-play.exe sets it
        });
        SetState("running");
        p.Exited += (o, e) => OnUi(() => SetState(""));
    }

    // A console window that stays open if the command fails, so its error can be read.
    static Process RunConsole(string command, string cwd, Action<ProcessStartInfo> env = null)
    {
        var psi = new ProcessStartInfo("cmd.exe", "/c \"" + command + " || (pause & exit /b 1)\"") { WorkingDirectory = cwd, UseShellExecute = false };
        // Own temp folder: this PC blocks some tools writing to %TEMP% (MSVC failed with D8050 building xcl).
        string tmp = Path.Combine(Launcher.Data, "tmp");
        Directory.CreateDirectory(tmp);
        psi.EnvironmentVariables["TEMP"] = psi.EnvironmentVariables["TMP"] = tmp;
        // Same for AppData and cargo's home: security software here (Malwarebytes, earlier Bitdefender) denies the launcher's
        // whole process tree writes there, which broke cargo downloads and VIctory-NVIDIA's logs. Toolchains in ~/.rustup are only read.
        psi.EnvironmentVariables["LOCALAPPDATA"] = Path.Combine(Launcher.Data, "appdata", "Local");
        psi.EnvironmentVariables["APPDATA"] = Path.Combine(Launcher.Data, "appdata", "Roaming");
        psi.EnvironmentVariables["CARGO_HOME"] = Path.Combine(Launcher.Data, "cargo");
        foreach (var k in new[] { "LOCALAPPDATA", "APPDATA", "CARGO_HOME" }) Directory.CreateDirectory(psi.EnvironmentVariables[k]);
        if (env != null) env(psi);
        var p = Process.Start(psi);
        p.EnableRaisingEvents = true;
        return p;
    }

    void OnUi(Action a)
    {
        if (IsHandleCreated && !IsDisposed) try { BeginInvoke(a); } catch (InvalidOperationException) { }
    }

    void SetState(string st) { state = st; PlayMusic(); Invalidate(); }

    // Music plays while the launcher is idle or installing; the game brings its own.
    void PlayMusic()
    {
        bool on = musicWav != null && !muted && state != "running";
        if (!on) { if (player != null) player.Stop(); return; }
        if (player == null) player = new System.Media.SoundPlayer(musicWav);
        player.PlayLooping();
    }

    protected override void OnFormClosed(FormClosedEventArgs e)
    {
        if (player != null) player.Stop();
        foreach (var b in new[] { shown, prevBmp, nextBmp }) if (b != IntPtr.Zero) DeleteObject(b);
        base.OnFormClosed(e);
    }

    public static string Visual(XmlDocument doc, string attr)
    {
        var n = doc == null ? null : doc.SelectSingleNode("//*[local-name()='ShellVisuals']/@" + attr);
        return n != null && n.Value.Length > 0 && !n.Value.StartsWith("ms-resource") ? n.Value : null;
    }

    public static string Backgrounds { get { return Path.Combine(Launcher.Here, "backgrounds"); } }
    static bool IsImage(string f) { return new[] { ".png", ".jpg", ".jpeg", ".bmp", ".gif" }.Contains(Path.GetExtension(f).ToLowerInvariant()); }

    // The game's splash (copied to victory-data so it shows even when the game folder isn't there), then its music
    // (decoded once in the background).
    void LoadGame(bool changed)
    {
        title = "GTA VI";
        try
        {
            Directory.CreateDirectory(Launcher.Data);
            string cached = Path.Combine(Launcher.Data, "art-splash.png");
            if (exe != null)
            {
                string dir = Path.GetDirectoryName(exe);
                var doc = Launcher.Manifest(dir);
                title = Visual(doc, "DefaultDisplayName") ?? title;
                string src = Visual(doc, "SplashScreenImage");
                if (src != null && File.Exists(Path.Combine(dir, src))) File.Copy(Path.Combine(dir, src), cached, true);
                else if (changed) File.Delete(cached);
            }
        }
        catch { } // a broken manifest just means no splash
        ListArt();
        ShowSlide(0);
        LoadMusic(changed);
    }

    // The chosen song, decoded once in the background (a few seconds) and cached in victory-data.
    void LoadMusic(bool fresh)
    {
        if (player != null) { player.Stop(); player.Dispose(); player = null; }
        musicWav = null;
        musicBusy = exe != null;
        if (exe == null) return;
        string gameDir = Path.GetDirectoryName(exe);
        int track = song;
        new Thread(() =>
        {
            string wav = GameMusic.Prepare(gameDir, track, fresh);
            OnUi(() =>
            {
                if (exe == null || Path.GetDirectoryName(exe) != gameDir || track != song || (wav == null && musicWav != null)) return; // a later pick won
                musicWav = wav;
                musicBusy = false;
                PlayMusic();
                Invalidate();
            });
        }) { IsBackground = true, Priority = ThreadPriority.BelowNormal }.Start();
    }

    // The slideshow: every picture in the backgrounds folder; without any, the game's splash; without that, the sunset.
    void ListArt()
    {
        art.Clear();
        try
        {
            if (Directory.Exists(Backgrounds))
                art.AddRange(Directory.GetFiles(Backgrounds).Where(IsImage).OrderBy(f => f, StringComparer.OrdinalIgnoreCase));
        }
        catch { }
        string splash = Path.Combine(Launcher.Data, "art-splash.png");
        if (art.Count == 0 && File.Exists(splash)) art.Add(splash);
        if (art.Count == 0) art.Add(null);
    }

    protected override void OnDragEnter(DragEventArgs e) { e.Effect = Dropped(e).Length > 0 ? DragDropEffects.Copy : DragDropEffects.None; }

    protected override void OnDragDrop(DragEventArgs e)
    {
        try
        {
            string first = null;
            Directory.CreateDirectory(Backgrounds);
            foreach (var f in Dropped(e))
            {
                string to = Path.Combine(Backgrounds, Path.GetFileName(f));
                if (!File.Exists(to)) File.Copy(f, to);
                if (first == null) first = to;
            }
            ListArt();
            ShowSlide(Math.Max(0, art.IndexOf(first)));
        }
        catch (Exception ex) { Launcher.Error(ex.Message); }
    }

    static string[] Dropped(DragEventArgs e)
    {
        var files = e.Data.GetData(DataFormats.FileDrop) as string[];
        return files == null ? new string[0] : files.Where(IsImage).ToArray();
    }

    // Shows slide i now, at the window's size, and composes the one after it in the background.
    void ShowSlide(int i)
    {
        foreach (var b in new[] { shown, prevBmp, nextBmp }) if (b != IntPtr.Zero) DeleteObject(b);
        prevBmp = nextBmp = IntPtr.Zero;
        slide = Math.Min(i, art.Count - 1);
        slideSize = ClientSize;
        shown = ComposeBitmap(slideSize, art[slide]);
        shownAt = clock.Elapsed.TotalSeconds;
        fadeFrom = -9;
        PrepareNext();
        Invalidate();
    }

    void PrepareNext()
    {
        if (art.Count < 2 || preparing || nextBmp != IntPtr.Zero) return;
        int i = (slide + 1) % art.Count;
        string path = art[i];
        var size = slideSize;
        preparing = true;
        new Thread(() =>
        {
            IntPtr b = ComposeBitmap(size, path);
            OnUi(() =>
            {
                preparing = false;
                // Stale if the window was resized or the list changed meanwhile.
                if (size != slideSize || nextBmp != IntPtr.Zero || art.Count < 2 || art[(slide + 1) % art.Count] != path) { DeleteObject(b); PrepareNext(); return; }
                nextBmp = b;
                nextIndex = i;
            });
        }) { IsBackground = true, Priority = ThreadPriority.BelowNormal }.Start();
    }

    IntPtr ComposeBitmap(Size size, string path)
    {
        Image src = null;
        try
        {
            if (path != null)
                using (var fs = File.OpenRead(path))
                using (var i = Image.FromStream(fs)) src = new Bitmap(i);
        }
        catch { src = null; } // unreadable picture: the sunset instead
        using (var bmp = Compose(size.Width, size.Height, src))
        {
            if (src != null) src.Dispose();
            return bmp.GetHbitmap();
        }
    }

    // One finished slide: the picture filling the window (cropped, not stretched), darkened under the controls.
    Bitmap Compose(int w, int h, Image src)
    {
        var bmp = new Bitmap(w, h, PixelFormat.Format32bppPArgb);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            if (src == null) Sunset(g, w, h);
            else Cover(g, src, w, h);

            int from = (int)(h * 0.5), mid = (int)(h * 0.76);
            Fade(g, new Rectangle(0, from, w, mid - from), Color.FromArgb(0, 8, 6, 14), Color.FromArgb(205, 8, 6, 14), 90f);
            Fade(g, new Rectangle(0, mid, w, h - mid), Color.FromArgb(205, 8, 6, 14), Color.FromArgb(245, 8, 6, 14), 90f);
            Fade(g, new Rectangle(0, 0, w, S(70)), Color.FromArgb(140, 0, 0, 0), Color.FromArgb(0, 0, 0, 0), 90f);
            if (src == null)
            {
                // Title only over the launcher's own art: the game's art carries its logo.
                Fade(g, new Rectangle(0, 0, (int)(w * 0.6), h), Color.FromArgb(150, 8, 6, 14), Color.FromArgb(0, 8, 6, 14), 0f);
                g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
                int x = Rail + S(36);
                using (var accent = new SolidBrush(Accent)) g.DrawString("PC PORT LAUNCHER", tagF, accent, x + S(2), h - S(214));
                using (var white = new SolidBrush(Color.White)) g.DrawString(title.ToUpperInvariant(), titleF, white, x - S(3), h - S(196));
            }
        }
        return bmp;
    }

    static void Cover(Graphics g, Image src, int w, int h)
    {
        float k = Math.Max((float)w / src.Width, (float)h / src.Height);
        g.DrawImage(src, (w - src.Width * k) / 2, (h - src.Height * k) / 2, src.Width * k, src.Height * k);
    }

    static void Fade(Graphics g, Rectangle r, Color a, Color b, float angle)
    {
        var br = Rectangle.Inflate(r, 1, 1); // avoids the 1px wrap line at gradient edges
        using (var brush = new LinearGradientBrush(br, a, b, angle)) g.FillRectangle(brush, r);
    }

    static Color C(int rgb) { return Color.FromArgb(255, Color.FromArgb(rgb)); }

    // Default artwork: dusk sky, striped sun, its reflection on the water.
    static void Sunset(Graphics g, int w, int h)
    {
        int hz = (int)(h * 0.64);
        using (var sky = new LinearGradientBrush(new Rectangle(0, -1, w, hz + 2), Color.Black, Color.Black, 90f))
        {
            sky.InterpolationColors = new ColorBlend
            {
                Colors = new[] { C(0x070513), C(0x260A3A), C(0x8A1F5C), C(0xFF6A3D) },
                Positions = new[] { 0f, .45f, .82f, 1f },
            };
            g.FillRectangle(sky, 0, 0, w, hz);
            var rnd = new Random(7);
            for (int i = 0; i < 90; i++)
                using (var star = new SolidBrush(Color.FromArgb(rnd.Next(40, 160), 255, 255, 255)))
                    g.FillEllipse(star, rnd.Next(w), rnd.Next(hz / 2), 1.6f, 1.6f);

            float r = h * 0.27f, cx = w * 0.70f, cy = hz - r * 0.2f;
            g.SetClip(new Rectangle(0, 0, w, hz));
            using (var sun = new LinearGradientBrush(new RectangleF(cx - r, cy - r - 1, 2 * r, 2 * r + 2), C(0xFFD36B), C(0xFF3D7F), 90f))
                g.FillEllipse(sun, cx - r, cy - r, 2 * r, 2 * r);
            for (int i = 0; i < 6; i++) // stripes cut with the sky brush, thicker toward the horizon
            {
                float y = cy - r * 0.55f + i * r * 0.14f, t = r * (0.015f + i * 0.014f);
                g.FillRectangle(sky, cx - r, y, 2 * r, t);
            }
            g.ResetClip();

            using (var sea = new LinearGradientBrush(new Rectangle(0, hz - 1, w, h - hz + 2), C(0x4A1040), C(0x0A0510), 90f))
                g.FillRectangle(sea, 0, hz, w, h - hz);
            Fade(g, new Rectangle(0, hz - (int)(h * 0.08f), w, (int)(h * 0.08f)), Color.FromArgb(0, 255, 120, 70), Color.FromArgb(110, 255, 120, 70), 90f);
            for (int i = 0; i < 14; i++)
            {
                float y = hz + 3 + i * i * 1.1f + i * 4, half = r * (1f - i * 0.06f) * (0.75f + (i % 3) * 0.12f);
                using (var line = new SolidBrush(Color.FromArgb(Math.Max(30, 235 - i * 14), 255, 150 + i * 3, 100)))
                    g.FillRectangle(line, cx - half, y, 2 * half, 2f + i * 0.25f);
            }
            using (var glow = new Pen(Color.FromArgb(180, 255, 154, 90))) g.DrawLine(glow, 0, hz, w, hz);
        }
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        int w = ClientSize.Width, h = ClientSize.Height;
        double t = clock.Elapsed.TotalSeconds;

        // Background: the current slide, crossfading from the previous one.
        var clip = Rectangle.Intersect(e.ClipRectangle, new Rectangle(Point.Empty, slideSize));
        double f = (t - fadeFrom) / FadeSeconds;
        IntPtr hdc = g.GetHdc(), mem = CreateCompatibleDC(hdc);
        bool fading = f < 1 && prevBmp != IntPtr.Zero;
        IntPtr old = SelectObject(mem, fading ? prevBmp : shown);
        BitBlt(hdc, clip.X, clip.Y, clip.Width, clip.Height, mem, clip.X, clip.Y, 0x00CC0020); // SRCCOPY
        if (fading)
        {
            SelectObject(mem, shown);
            int a = (int)(255 * f * f * (3 - 2 * f));
            AlphaBlend(hdc, clip.X, clip.Y, clip.Width, clip.Height, mem, clip.X, clip.Y, clip.Width, clip.Height, a << 16);
        }
        SelectObject(mem, old);
        DeleteDC(mem);
        g.ReleaseHdc(hdc);
        g.SmoothingMode = SmoothingMode.AntiAlias;
        bool onlyPlay = Glow.Contains(e.ClipRectangle); // a glow frame: the rest of the window is unchanged
        var cur = Current;

        g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
        var muted2 = Color.FromArgb(185, 180, 200);

        if (!onlyPlay)
        {
            hits.Clear();
            // Side tabs; the other tabs dim the art so their text reads.
            using (var b = new SolidBrush(Color.FromArgb(150, 8, 6, 14))) g.FillRectangle(b, 0, 0, Rail, h);
            if (tab != 0) using (var b = new SolidBrush(Color.FromArgb(175, 8, 6, 14))) g.FillRectangle(b, Rail, 0, w - Rail, h);
            g.DrawIcon(smallIcon, new Rectangle((Rail - S(16)) / 2, S(9), S(16), S(16)));
            var center = TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine;
            for (int i = 0; i < Tabs.Length; i++)
            {
                var r = new Rectangle(0, S(48) + i * S(74), Rail, S(70));
                hits["tab" + i] = r;
                bool on = tab == i;
                if (on || hover == "tab" + i) using (var b = new SolidBrush(Color.FromArgb(on ? 40 : 22, 255, 255, 255))) g.FillRectangle(b, r);
                if (on) using (var b = new SolidBrush(Accent)) g.FillRectangle(b, 0, r.Y + S(14), S(3), r.Height - S(28));
                var c = on ? Color.White : muted2;
                TextRenderer.DrawText(g, TabGlyphs[i], tabF, new Rectangle(0, r.Y + S(8), Rail, S(30)), c, center);
                TextRenderer.DrawText(g, Tabs[i], smallF, new Rectangle(0, r.Y + S(42), Rail, S(18)), c, center);
            }

            // Top bar: app name, music, minimize, full screen, close.
            TextRenderer.DrawText(g, "GTA VI LAUNCHER", smallF, new Point(Rail + S(14), S(9)), Color.FromArgb(220, 255, 255, 255));
            if (newer != null)
            {
                string up = "Launcher " + newer.ToString(3) + " is out  ·  Update";
                int ux = Rail + S(28) + TextRenderer.MeasureText("GTA VI LAUNCHER", smallF).Width;
                var ur = hits["app-update"] = new Rectangle(ux, S(5), TextRenderer.MeasureText(up, smallF).Width + S(20), S(24));
                using (var path = Rounded(ur, ur.Height / 2))
                using (var b = new SolidBrush(hover == "app-update" ? Color.FromArgb(255, 120, 162) : Accent))
                    g.FillPath(b, path);
                TextRenderer.DrawText(g, up, smallF, ur, Color.White, center);
            }
            using (var hot = new SolidBrush(Color.FromArgb(40, 255, 255, 255)))
            {
                if (hover == "min") g.FillRectangle(hot, minR);
                if (hover == "max") g.FillRectangle(hot, maxR);
                if (hover == "mute") g.FillRectangle(hot, muteR);
            }
            if (hover == "close") using (var b = new SolidBrush(Color.FromArgb(232, 17, 35))) g.FillRectangle(b, closeR);
            using (var pen = new Pen(Color.White, Math.Max(1f, s)))
            {
                int k = S(5);
                g.SmoothingMode = SmoothingMode.None;
                int mx = minR.X + minR.Width / 2, my = minR.Y + minR.Height / 2;
                g.DrawLine(pen, mx - k, my, mx + k, my);
                int xx = maxR.X + maxR.Width / 2, xy = maxR.Y + maxR.Height / 2;
                g.DrawRectangle(pen, xx - k, xy - k, 2 * k, 2 * k);
                if (WindowState == FormWindowState.Maximized) g.DrawLine(pen, xx - k + S(2), xy - k - S(2), xx + k + S(2), xy - k - S(2));
                g.SmoothingMode = SmoothingMode.AntiAlias;
                int cx = closeR.X + closeR.Width / 2, cy = closeR.Y + closeR.Height / 2;
                g.DrawLine(pen, cx - k, cy - k, cx + k, cy + k);
                g.DrawLine(pen, cx + k, cy - k, cx - k, cy + k);
            }
            if (musicWav != null)
                TextRenderer.DrawText(g, muted ? "" : "", iconF, muteR, muted ? muted2 : Color.White, // Mute / Volume glyphs
                    TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine);

            int x = Rail + S(36);
            if (tab == 1) ConversionPage(g, x, w, cur, muted2);
            else if (tab == 2) SettingsPage(g, x, w, muted2);
            else
            {
                // Game path and Change.
                string line = exe ?? "Select your game's .exe to get started";
                var flags = TextFormatFlags.PathEllipsis | TextFormatFlags.SingleLine | TextFormatFlags.NoPadding;
                int maxW = playR.X - x - S(110);
                int lw = Math.Min(TextRenderer.MeasureText(line, pathF, Size.Empty, TextFormatFlags.SingleLine | TextFormatFlags.NoPadding).Width, maxW);
                var pathR = new Rectangle(x, h - S(112), lw, pathF.Height + S(4));
                TextRenderer.DrawText(g, line, pathF, pathR, muted2, flags);
                if (exe != null)
                {
                    var changeR = hits["change"] = new Rectangle(pathR.Right + S(14), pathR.Y - S(4), TextRenderer.MeasureText("Change", pathF).Width, pathR.Height + S(8));
                    TextRenderer.DrawText(g, "Change", hover == "change" ? linkF : pathF,
                        new Point(changeR.X, pathR.Y), hover == "change" ? Color.White : Accent, TextFormatFlags.NoPadding);
                }

                // The chosen conversion tool: its name and the cards it's made for. The Conversion tab picks another.
                int nw = TextRenderer.MeasureText(cur.Name, pillF).Width;
                var r = new Rectangle(x, h - S(80), nw + S(18) + (cur.Gpu != null ? GpuW() + S(4) : 0), S(34));
                using (var path = Rounded(r, r.Height / 2))
                using (var b = new SolidBrush(Color.FromArgb(235, 255, 255, 255)))
                    g.FillPath(b, path);
                TextRenderer.DrawText(g, cur.Name, pillF, new Rectangle(r.X + S(9), r.Y, nw, r.Height), Color.FromArgb(20, 16, 30), center);
                GpuBadge(g, cur.Gpu, r.X + S(9) + nw + S(6), r.Y + r.Height / 2);
            }
        }

        // PLAY, with a soft pulsing glow while it's ready. Launcher tab only.
        if (tab == 0) DrawPlay(g, t, cur);

        // Fade in on start.
        if (t < 0.6)
            using (var b = new SolidBrush(Color.FromArgb((int)(255 * (1 - t / 0.6)), Color.Black)))
                g.FillRectangle(b, ClientRectangle);
    }

    // Every runtime as a row: rank, name, how far it gets, installed or not. Click one to use it.
    void ConversionPage(Graphics g, int x, int w, Runtime cur, Color muted2)
    {
        TextRenderer.DrawText(g, "CONVERSION TOOL", tagF, new Point(x, S(52)), Accent);
        TextRenderer.DrawText(g, "The PC port runtime PLAY runs the game with. #1 to #3 are the archive's current top 3.", pathF, new Point(x, S(74)), muted2);
        int y = S(108), rw = w - x - S(40);
        var mid = TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine | TextFormatFlags.EndEllipsis;
        // Rows shrink to fit the window. shortcut: no scrolling; past ~10 runtimes rows get too thin, add a scroll offset then.
        int step = Math.Min(S(56), (ClientSize.Height - y - S(12)) / Math.Max(1, runtimes.Count));
        // The name column fits the longest name (the site's own names, e.g. "NEWEST CGBJ private"), within reason.
        int nameW = Math.Min(S(230), runtimes.Max(rt => TextRenderer.MeasureText(rt.Name, pillF).Width));
        for (int i = 0; i < runtimes.Count; i++, y += step)
        {
            var rt = runtimes[i];
            var r = new Rectangle(x, y, rw, step - S(6));
            // Its build picker, like the site's download picker: click to switch build (registered first, so it wins over the row).
            var ob = Rectangle.Empty;
            if (rt.Choices > 1)
            {
                string ol = rt.ChoiceLabel + "  ▾";
                int ow = TextRenderer.MeasureText(ol, smallF).Width + S(20);
                ob = new Rectangle(r.Right - S(132) - ow, r.Y + S(8), ow, r.Height - S(16));
                hits["opt" + i] = ob;
            }
            hits["row" + i] = r;
            bool on = rt == cur, hot = hover == "row" + i;
            using (var path = Rounded(r, S(8)))
            using (var b = new SolidBrush(on ? Color.FromArgb(235, 255, 255, 255) : Color.FromArgb(hot ? 60 : 30, 255, 255, 255)))
                g.FillPath(b, path);
            Color fg = on ? Color.FromArgb(20, 16, 30) : Color.White, sub = on ? Color.FromArgb(90, 80, 110) : muted2;
            if (rt.Rank > 0) TextRenderer.DrawText(g, "#" + rt.Rank, pillF, new Rectangle(r.X + S(16), r.Y, S(40), r.Height), Accent, mid);
            TextRenderer.DrawText(g, rt.Name, pillF, new Rectangle(r.X + S(60), r.Y, nameW, r.Height), fg, mid);
            GpuBadge(g, rt.Gpu, r.X + S(68) + nameW, r.Y + r.Height / 2);
            int noteL = r.X + S(80) + nameW + GpuW(), noteR = ob.IsEmpty ? r.Right - S(130) : ob.X - S(10);
            TextRenderer.DrawText(g, rt.Note, pathF, new Rectangle(noteL, r.Y, noteR - noteL, r.Height), sub, mid);
            if (!ob.IsEmpty)
            {
                using (var path = Rounded(ob, S(6)))
                using (var b = new SolidBrush(on ? Color.FromArgb(hover == "opt" + i ? 50 : 28, 20, 16, 30) : Color.FromArgb(hover == "opt" + i ? 70 : 38, 255, 255, 255)))
                    g.FillPath(b, path);
                TextRenderer.DrawText(g, rt.ChoiceLabel + "  ▾", smallF, ob, fg, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine);
            }
            bool old = rt.Installed && outdated.Contains(rt.Dir);
            TextRenderer.DrawText(g, old ? "Update available" : on ? "Selected" : rt.Installed ? "Installed" : "Not installed", smallF,
                new Rectangle(r.Right - S(126), r.Y, S(110), r.Height), on || old ? Accent : sub, mid | TextFormatFlags.Right);
        }
    }

    // NVIDIA green or AMD red tag, left edge at x, centred on cy. Every tag is as wide as "NVIDIA", so the notes line up.
    int GpuW() { return TextRenderer.MeasureText("NVIDIA", smallF).Width + S(12); }
    void GpuBadge(Graphics g, string gpu, int x, int cy)
    {
        if (gpu == null) return;
        bool nv = gpu == "nvidia";
        var r = new Rectangle(x, cy - S(11), GpuW(), S(22));
        using (var path = Rounded(r, S(5)))
        using (var b = new SolidBrush(nv ? Color.FromArgb(118, 185, 0) : Color.FromArgb(237, 28, 36)))
            g.FillPath(b, path);
        TextRenderer.DrawText(g, nv ? "NVIDIA" : "AMD", smallF, r, Color.White, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine);
    }

    void SettingsPage(Graphics g, int x, int w, Color muted2)
    {
        TextRenderer.DrawText(g, "SETTINGS", tagF, new Point(x, S(52)), Accent);
        int vx = x + S(130), bx = w - S(40), y = S(96);
        var mid = TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine | TextFormatFlags.NoPadding;
        Action<string, string> row = (label, value) =>
        {
            TextRenderer.DrawText(g, label, pillF, new Rectangle(x, y, S(120), S(40)), Color.White, mid);
            TextRenderer.DrawText(g, value, pathF, new Rectangle(vx, y, bx - vx - S(250), S(40)), muted2, mid | TextFormatFlags.PathEllipsis);
        };

        row("Game", exe ?? "Not selected yet");
        Button(g, "change", new Rectangle(bx - S(110), y + S(3), S(110), S(34)), exe == null ? "Select" : "Change");

        y += S(60);
        string n = "Song " + (song + 1) + " of " + GameMusic.Songs.Length;
        row("Music", exe == null ? "Select the game first" : musicBusy ? n + "  ·  loading..." : musicWav == null ? n + "  ·  not found in this game version" : muted ? n + "  ·  muted" : n);
        Button(g, "mute", new Rectangle(bx - S(110), y + S(3), S(110), S(34)), muted ? "Unmute" : "Mute");
        Button(g, "song+", new Rectangle(bx - S(160), y + S(3), S(42), S(34)), "›");
        Button(g, "song-", new Rectangle(bx - S(208), y + S(3), S(42), S(34)), "‹");

        y += S(60);
        int pics = art.Count(a => a != null && a.StartsWith(Backgrounds, StringComparison.OrdinalIgnoreCase));
        row("Pictures", pics + (pics == 1 ? " picture" : " pictures") + "  ·  drop more on the window");
        Button(g, "folder", new Rectangle(bx - S(130), y + S(3), S(130), S(34)), "Open folder");

        y += S(60);
        row("Full screen", "F11, Alt+Enter or double-click the window; Esc to leave");
        TextRenderer.DrawText(g, "v" + Assembly.GetExecutingAssembly().GetName().Version.ToString(3), smallF, new Point(x, ClientSize.Height - S(36)), muted2);
    }

    void Button(Graphics g, string key, Rectangle r, string text)
    {
        hits[key] = r;
        using (var path = Rounded(r, S(6)))
        {
            using (var b = new SolidBrush(Color.FromArgb(hover == key ? 70 : 38, 255, 255, 255))) g.FillPath(b, path);
            using (var pen = new Pen(Color.FromArgb(60, 255, 255, 255))) g.DrawPath(pen, path);
        }
        TextRenderer.DrawText(g, text, pillF, r, Color.White, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine);
    }

    void DrawPlay(Graphics g, double t, Runtime cur)
    {
        bool ready = state == "";
        string label2 = state == "installing" ? "INSTALLING..." : state == "updating" ? "UPDATING..." : state == "running" ? "RUNNING" :
            exe == null ? "SELECT GAME" : !cur.Installed ? "INSTALL & PLAY" : outdated.Contains(cur.Dir) ? "UPDATE" : "PLAY";
        if (ready)
        {
            float pulse = 0.5f + 0.5f * (float)Math.Sin(t * 2 * Math.PI / 2.6);
            for (int i = 1; i <= 3; i++)
                using (var path = Rounded(Rectangle.Inflate(playR, S(2 * i), S(2 * i)), S(8 + 2 * i)))
                using (var b = new SolidBrush(Color.FromArgb((int)((14 + 18 * pulse) / i), Accent)))
                    g.FillPath(b, path);
        }
        Color fill = !ready ? Color.FromArgb(90, 255, 255, 255) : hover == "play" ? Color.FromArgb(255, 120, 162) : Accent;
        using (var path = Rounded(playR, S(8)))
        using (var b = new SolidBrush(fill))
            g.FillPath(b, path);
        TextRenderer.DrawText(g, label2, label2.Length > 10 ? pillF : playF, playR, Color.White,
            TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine);
    }

    static GraphicsPath Rounded(Rectangle r, int rad)
    {
        int d = Math.Max(1, rad * 2);
        var p = new GraphicsPath();
        p.AddArc(r.X, r.Y, d, d, 180, 90);
        p.AddArc(r.Right - d, r.Y, d, d, 270, 90);
        p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
        p.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
        p.CloseFigure();
        return p;
    }
}

// Game music straight from the game's own audio.rpf: its AWC sound banks are stored uncompressed, music as streamed
// mono Opus channels, decoded here with the Opus decoder that ships with Windows (MSOpusDecoder.dll, a Media
// Foundation transform). Bank layout follows rpf9-explorer's Tools/OpusPreview.cpp.
static class GameMusic
{
    // shortcut: songs are located by their bank's {offset, length} in the v1.0.2.1 dump (the streamed-music banks with the
    // strongest beat; channel 0 is the music), other dumps just get no music; scan audio.rpf for "ADAT" banks if another build turns up.
    public static readonly long[][] Songs =
    {
        new[] { 0x213135000L, 86258320L }, new[] { 0x12C3AC000L, 103261488L }, new[] { 0x136B58000L, 62957360L },
        new[] { 0x132FBB000L, 55592144L }, new[] { 0x15034A000L, 260816000L }, // the last one was v1.4's song
    };
    const int Seconds = 180, Rate = 24000;
    public static string Wav(int song) { return Path.Combine(Launcher.Data, "music-" + Songs[song][0].ToString("X") + ".wav"); }

    // The decoded song, made on first use; null when this game has none we can read.
    public static string Prepare(string gameDir, int song, bool fresh)
    {
        try
        {
            string wav = Wav(song);
            if (fresh) File.Delete(wav);
            File.Delete(Path.Combine(Launcher.Data, "music.wav")); // v1.4's cache
            if (File.Exists(wav)) return wav;
            string rpf = Path.Combine(gameDir, "audio.rpf");
            if (!File.Exists(rpf) || new FileInfo(rpf).Length < Songs[song][0] + Songs[song][1]) return null;
            Directory.CreateDirectory(Launcher.Data);
            string part = wav + ".part";
            using (var f = new FileStream(rpf, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 1 << 16))
                WriteWav(part, Decode(f, Inspect(f, Songs[song][0])[0], Seconds * 48000));
            File.Delete(wav);
            File.Move(part, wav);
            return wav;
        }
        catch { return null; } // music is a nicety; any failure just means silence
    }

    // 48 kHz float -> 24 kHz 16-bit mono, normalised, with a 2 s fade at each end so the loop point is soft.
    public static void WriteWav(string path, float[] x)
    {
        int n = x.Length / 2;
        float peak = 1e-6f;
        foreach (float v in x) peak = Math.Max(peak, Math.Abs(v));
        float gain = Math.Min(0.8f / peak, 8f);
        var pcm = new byte[n * 2];
        for (int i = 0; i < n; i++)
        {
            float env = Math.Min(1f, Math.Min(i, n - 1 - i) / (2f * Rate));
            short v = (short)(Math.Max(-1f, Math.Min(1f, (x[2 * i] + x[2 * i + 1]) / 2 * gain * env)) * 32767);
            pcm[2 * i] = (byte)v; pcm[2 * i + 1] = (byte)(v >> 8);
        }
        using (var o = new BinaryWriter(File.Create(path)))
        {
            o.Write(0x46464952); o.Write(36 + pcm.Length); o.Write(0x45564157); o.Write(0x20746d66); o.Write(16); // RIFF, WAVE, fmt
            o.Write((short)1); o.Write((short)1); o.Write(Rate); o.Write(Rate * 2); o.Write((short)2); o.Write((short)16);
            o.Write(0x61746164); o.Write(pcm.Length); o.Write(pcm);                                              // data
        }
    }

    static byte[] Read(Stream f, long at, int n)
    {
        var b = new byte[n];
        f.Position = at;
        for (int got = 0; got < n; )
        {
            int k = f.Read(b, got, n - got);
            if (k <= 0) throw new EndOfStreamException();
            got += k;
        }
        return b;
    }

    // Per music channel of the bank's first streamed-music sound: its Opus data as {offset, bytes, packets' samples to skip}.
    public static List<long[]>[] Inspect(Stream f, long bank)
    {
        var pre = Read(f, bank, 16);
        uint flags = BitConverter.ToUInt32(pre, 4), n = BitConverter.ToUInt32(pre, 8), headerSize = BitConverter.ToUInt32(pre, 12);
        if (BitConverter.ToUInt32(pre, 0) != 0x54414441 || (flags & 0xFF00FFFF) != 0xFF000001 || n == 0 || n > 8192 || headerSize > 4 << 20)
            throw new InvalidDataException("Not an AWC bank."); // "ADAT"
        var h = Read(f, bank, (int)headerSize);
        long p = 16 + ((flags & 0x400000) != 0 ? 4 : 0);
        if ((flags & 0x10000) != 0) p += n * 2;
        if ((flags & 0x200000) != 0) p += (n + 1) * 4;
        long info = p;
        p += n * 4;
        for (int i = 0; i < n; i++)
        {
            uint v = BitConverter.ToUInt32(h, (int)(info + i * 4));
            long fmt = 0, fmtSize = 0, data = 0, dataSize = 0;
            for (int j = 0; j < (v >> 29); j++, p += 8)
            {
                if (p + 8 > h.Length) throw new InvalidDataException("AWC tag table out of bounds.");
                ulong tag = BitConverter.ToUInt64(h, (int)p);
                long type = (long)(tag >> 56), size = (long)((tag >> 28) & 0x0FFFFFFF), at = (long)(tag & 0x0FFFFFFF);
                if (type == 0x55) { data = at; dataSize = size; }  // data
                if (type == 0x48) { fmt = at; fmtSize = size; }    // streamed music format
            }
            if (fmt == 0 || dataSize == 0) continue;
            if (fmtSize < 12 || fmtSize > 524) throw new InvalidDataException("Bad music format.");
            var m = Read(f, bank + fmt, (int)fmtSize);
            uint blocks = BitConverter.ToUInt32(m, 0), chunk = BitConverter.ToUInt32(m, 4), channels = BitConverter.ToUInt32(m, 8);
            if (channels == 0 || channels > 32 || m.Length != 12 + channels * 16 || chunk < 2048 || blocks == 0 || (long)blocks * chunk < dataSize)
                throw new InvalidDataException("Bad music block layout.");
            var spans = new List<long[]>[channels];
            for (int c = 0; c < channels; c++)
            {
                if (m[12 + c * 16 + 12] != 0x0B) throw new InvalidDataException("Music isn't Opus.");
                spans[c] = new List<long[]>();
            }
            for (long b = 0; b < blocks; b++)
            {
                long at = bank + data + b * chunk;
                var rec = Read(f, at, (int)channels * 24);
                long total = 0, before = 0;
                for (int c = 0; c < channels; c++) total += BitConverter.ToUInt32(rec, c * 24 + 4);
                long header = (channels * 24 + total * 4 + 2047) & ~2047L;
                for (int c = 0; c < channels; c++)
                {
                    long count = BitConverter.ToUInt32(rec, c * 24 + 4);
                    if (count > 0) spans[c].Add(new[] { at + header + before * 2048, count * 2048, BitConverter.ToUInt32(rec, c * 24 + 8) });
                    before += count;
                }
            }
            return spans;
        }
        throw new InvalidDataException("No streamed music in this bank.");
    }

    // Up to `frames` samples (48 kHz mono float) of one channel. Packets sit in 2048-byte blocks behind a u16 length;
    // each block may begin with packets repeated from the previous one, which its skip count covers.
    public static float[] Decode(Stream f, List<long[]> spans, int frames)
    {
        var into = new float[frames];
        int w = 0;
        using (var opus = new OpusDecoder())
            foreach (var s in spans)
            {
                long skip = s[2];
                for (long b = 0; b < s[1] && w < frames; b += 2048)
                {
                    var block = Read(f, s[0] + b, 2048);
                    for (int p = 0; p + 2 <= 2048 && w < frames; )
                    {
                        int bytes = block[p] | block[p + 1] << 8;
                        if (bytes == 0 || bytes > 2046 - p) break; // padding
                        if (skip > 0) skip -= OpusDecoder.Samples(block, p + 2, bytes);
                        else w = opus.Decode(block, p + 2, bytes, into, w);
                        p += 2 + bytes;
                    }
                }
                if (w >= frames) break;
            }
        if (w < frames) Array.Resize(ref into, w);
        return into;
    }
}

// One mono 48 kHz Opus stream through Windows' Opus decoder MFT.
sealed class OpusDecoder : IDisposable
{
    static readonly Guid MajorType = new Guid("48eba18e-f8c9-4687-bf11-0a74c9f96a8f"), SubType = new Guid("f7e34c9a-42e8-4714-b74b-cb29d72c35e5"),
        Audio = new Guid("73647561-0000-0010-8000-00AA00389B71"), OpusFormat = new Guid("0000704F-0000-0010-8000-00AA00389B71"),
        Float = new Guid("00000003-0000-0010-8000-00AA00389B71"), Pcm = new Guid("00000001-0000-0010-8000-00AA00389B71"),
        NumChannels = new Guid("37e48bf5-645e-4c5b-89de-ada9e29b696a"), SampleRate = new Guid("5faeeae7-0290-4c31-9e8a-c534f68d9dba"),
        UserData = new Guid("b6bc765f-4c3b-40a4-bd51-2535b66fe09d");
    static bool started;
    readonly IMFTransform t;
    readonly bool isFloat, provides;
    readonly int outSize;

    public OpusDecoder()
    {
        if (!started) { Check(MFStartup(0x20070, 0)); started = true; }
        t = (IMFTransform)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("63e17c10-2d43-4c42-8fe3-8d8b63e46a6a")));
        IMFMediaType input;
        Check(MFCreateMediaType(out input));
        input.SetGUID(MajorType, Audio);
        input.SetGUID(SubType, OpusFormat);
        input.SetUINT32(NumChannels, 1);
        input.SetUINT32(SampleRate, 48000);
        var head = new byte[] { 0x4F, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64, 1, 1, 0, 0, 0x80, 0xBB, 0, 0, 0, 0, 0 }; // OpusHead: v1, mono, 48 kHz
        input.SetBlob(UserData, head, head.Length);
        Check(t.SetInputType(0, input, 0));
        Marshal.ReleaseComObject(input);
        for (int i = 0; ; i++)
        {
            IMFMediaType o;
            Check(t.GetOutputAvailableType(0, i, out o));
            Guid sub;
            o.GetGUID(SubType, out sub);
            bool ok = sub == Float || sub == Pcm;
            if (ok) { isFloat = sub == Float; Check(t.SetOutputType(0, o, 0)); }
            Marshal.ReleaseComObject(o);
            if (ok) break;
        }
        StreamInfo osi;
        Check(t.GetOutputStreamInfo(0, out osi));
        provides = (osi.Flags & 0x300) != 0; // the MFT allocates its own output samples
        outSize = Math.Max(osi.Size, 5760 * 4);
        t.ProcessMessage(0x10000000, IntPtr.Zero); // MFT_MESSAGE_NOTIFY_BEGIN_STREAMING
    }

    // Samples in one Opus packet at 48 kHz (RFC 6716, section 3.1).
    public static int Samples(byte[] b, int p, int n)
    {
        int cfg = b[p] >> 3, code = b[p] & 3;
        int per = cfg < 12 ? new[] { 480, 960, 1920, 2880 }[cfg & 3] : cfg < 16 ? ((cfg & 1) == 0 ? 480 : 960) : new[] { 120, 240, 480, 960 }[cfg & 3];
        return per * (code == 0 ? 1 : code == 3 ? (n > 1 ? b[p + 1] & 0x3F : 0) : 2);
    }

    // Decodes one packet and adds it into `into` from `w`; returns the new write position.
    public int Decode(byte[] src, int at, int n, float[] into, int w)
    {
        IMFMediaBuffer buf;
        IMFSample sample;
        IntPtr ptr;
        int max, cur;
        Check(MFCreateMemoryBuffer(n, out buf));
        buf.Lock(out ptr, out max, out cur);
        Marshal.Copy(src, at, ptr, n);
        buf.Unlock();
        buf.SetCurrentLength(n);
        Check(MFCreateSample(out sample));
        sample.AddBuffer(buf);
        Check(t.ProcessInput(0, sample, 0));
        Marshal.ReleaseComObject(sample);
        Marshal.ReleaseComObject(buf);
        while (true)
        {
            IMFSample mine = null;
            IMFMediaBuffer mineBuf = null;
            if (!provides)
            {
                Check(MFCreateMemoryBuffer(outSize, out mineBuf));
                Check(MFCreateSample(out mine));
                mine.AddBuffer(mineBuf);
            }
            var od = new OutputData { Sample = mine == null ? IntPtr.Zero : Marshal.GetIUnknownForObject(mine) };
            int status, hr = t.ProcessOutput(0, 1, ref od, out status);
            IMFSample got = mine;
            if (od.Sample != IntPtr.Zero)
            {
                if (mine == null && hr >= 0) got = (IMFSample)Marshal.GetObjectForIUnknown(od.Sample);
                Marshal.Release(od.Sample);
            }
            if (hr == unchecked((int)0xC00D6D72)) { Release(got, mineBuf); return w; } // MF_E_TRANSFORM_NEED_MORE_INPUT
            Check(hr);
            IMFMediaBuffer cb;
            Check(got.ConvertToContiguousBuffer(out cb));
            cb.Lock(out ptr, out max, out cur);
            if (isFloat)
            {
                var tmp = new float[cur / 4];
                Marshal.Copy(ptr, tmp, 0, tmp.Length);
                for (int i = 0; i < tmp.Length && w < into.Length; i++) into[w++] += tmp[i];
            }
            else
            {
                var tmp = new short[cur / 2];
                Marshal.Copy(ptr, tmp, 0, tmp.Length);
                for (int i = 0; i < tmp.Length && w < into.Length; i++) into[w++] += tmp[i] / 32768f;
            }
            cb.Unlock();
            Release(cb, got, mineBuf);
        }
    }

    static void Release(params object[] o) { foreach (var x in o) if (x != null) Marshal.ReleaseComObject(x); }
    public void Dispose() { Marshal.ReleaseComObject(t); }
    static void Check(int hr) { if (hr < 0) Marshal.ThrowExceptionForHR(hr); }

    [StructLayout(LayoutKind.Sequential)] struct StreamInfo { public int Flags, Size, Alignment; }
    [StructLayout(LayoutKind.Sequential)] struct OutputData { public int StreamID; public IntPtr Sample; public int Status; public IntPtr Events; }

    [DllImport("mfplat.dll")] static extern int MFStartup(int version, int flags);
    [DllImport("mfplat.dll")] static extern int MFCreateMediaType(out IMFMediaType t);
    [DllImport("mfplat.dll")] static extern int MFCreateSample(out IMFSample s);
    [DllImport("mfplat.dll")] static extern int MFCreateMemoryBuffer(int max, out IMFMediaBuffer b);

    // Media Foundation interfaces: only the methods used are typed, the x* entries hold their vtable slots.
    [ComImport, Guid("44ae0fa8-ea31-4109-8d2e-4cae4997c555"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMFMediaType
    {
        void x0(); void x1(); void x2(); void x3(); void x4(); void x5(); void x6();
        [PreserveSig] int GetGUID([In, MarshalAs(UnmanagedType.LPStruct)] Guid key, out Guid value);
        void x8(); void x9(); void x10(); void x11(); void x12(); void x13(); void x14(); void x15(); void x16(); void x17();
        [PreserveSig] int SetUINT32([In, MarshalAs(UnmanagedType.LPStruct)] Guid key, int value);
        void x19(); void x20();
        [PreserveSig] int SetGUID([In, MarshalAs(UnmanagedType.LPStruct)] Guid key, [In, MarshalAs(UnmanagedType.LPStruct)] Guid value);
        void x22();
        [PreserveSig] int SetBlob([In, MarshalAs(UnmanagedType.LPStruct)] Guid key, [In, MarshalAs(UnmanagedType.LPArray)] byte[] buf, int size);
    }

    [ComImport, Guid("c40a00f2-b93a-4d80-ae8c-5a1c634f58e4"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMFSample
    {
        void x0(); void x1(); void x2(); void x3(); void x4(); void x5(); void x6(); void x7(); void x8(); void x9();
        void x10(); void x11(); void x12(); void x13(); void x14(); void x15(); void x16(); void x17(); void x18(); void x19();
        void x20(); void x21(); void x22(); void x23(); void x24(); void x25(); void x26(); void x27(); void x28(); void x29();
        void y0(); void y1(); void y2(); void y3(); void y4(); void y5(); void y6(); void y7();
        [PreserveSig] int ConvertToContiguousBuffer(out IMFMediaBuffer b);
        [PreserveSig] int AddBuffer(IMFMediaBuffer b);
    }

    [ComImport, Guid("045FA593-8799-42b8-BC8D-8968C6453507"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMFMediaBuffer
    {
        [PreserveSig] int Lock(out IntPtr p, out int max, out int cur);
        [PreserveSig] int Unlock();
        void x2();
        [PreserveSig] int SetCurrentLength(int n);
    }

    [ComImport, Guid("bf94c121-5b05-4e6f-8000-ba598961414d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMFTransform
    {
        void x0(); void x1(); void x2(); void x3();
        [PreserveSig] int GetOutputStreamInfo(int id, out StreamInfo info);
        void x5(); void x6(); void x7(); void x8(); void x9(); void x10();
        [PreserveSig] int GetOutputAvailableType(int id, int index, out IMFMediaType t);
        [PreserveSig] int SetInputType(int id, IMFMediaType t, int flags);
        [PreserveSig] int SetOutputType(int id, IMFMediaType t, int flags);
        void x14(); void x15(); void x16(); void x17(); void x18(); void x19();
        [PreserveSig] int ProcessMessage(int msg, IntPtr param);
        [PreserveSig] int ProcessInput(int id, IMFSample s, int flags);
        [PreserveSig] int ProcessOutput(int flags, int count, ref OutputData buffer, out int status);
    }
}
