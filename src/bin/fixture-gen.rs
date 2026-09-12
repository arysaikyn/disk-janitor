//! `fixture-gen` — synthetic junk-shape trees for tests/demos (SPEC §7).
//! Builds node_modules+lockfile, orphan .venv, DerivedData shape, _cacache CAS,
//! pnpm hardlinks, sparse file, symlink chain+loop — all under the target root.
//! Never pointed at real user data.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// Root directory to build fixtures under (created; must be empty or new).
    root: PathBuf,
    /// Which fixture set to build: a set name, or `all` (default: all).
    #[arg(long)]
    set: Option<String>,
    /// Multiply per-fixture file counts (default 1; demos size trees with this).
    #[arg(long, default_value_t = 1)]
    scale: u32,
    /// Allow building into an existing non-empty root or outside /tmp.
    #[arg(long)]
    force: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    fixture_gen::build(&args.root, args.set.as_deref(), args.scale, args.force)?;
    println!("fixtures built under {}", args.root.display());
    Ok(())
}

pub mod fixture_gen {
    use std::fs::File;
    use std::io::{Seek, SeekFrom, Write};
    use std::path::{Component, Path};
    use std::time::{Duration, SystemTime};

    use anyhow::{Context, Result, bail};

    const FILLER: u8 = b'x'; // deterministic content: exact byte sizes, byte-identical re-runs
    const CHUNK: usize = 64 * 1024;
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    /// now − 2h: comfortably outside the 600s hot-mtime window (spec says cold trees must
    /// NOT surface as `MtimeHot`).
    const COLD_SECS: u64 = 2 * 3600;

    #[cfg(target_os = "macos")]
    const UF_HIDDEN: u32 = 0x0000_8000;
    #[cfg(not(target_os = "macos"))]
    const UF_HIDDEN: u32 = 0;

    // ------------------------------------------------------------------ types

    struct FixtureReport {
        name: &'static str,
        files: u64,
        alloc: u64,
    }

    type Builder = fn(&Path, u32) -> Result<FixtureReport>;

    /// The 10 fixture sets (SPEC §7). Names are the `--set` values.
    const FIXTURES: &[(&str, Builder)] = &[
        ("demo-app", build_demo_app),
        ("orphan-node", build_orphan_node),
        ("venv", build_venv),
        ("derived-data", build_derived_data),
        ("cacache", build_cacache),
        ("pnpm-store", build_pnpm_store),
        ("sparse", build_sparse),
        ("symlink-chain", build_symlink_chain),
        ("trash-janitor", build_trash_janitor),
        ("hot-project", build_hot_project),
    ];

    // ------------------------------------------------------------------ entry

    /// Build one fixture set (or all) under `root`. `scale` multiplies per-fixture file
    /// counts; `force` bypasses the /tmp-only and non-empty-root refusals (never `/`).
    pub fn build(root: &Path, set: Option<&str>, scale: u32, force: bool) -> Result<()> {
        validate_root(root, force)?;
        std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
        let scale = scale.max(1);

        let picked: Vec<(&str, Builder)> = match set {
            // `--set all` and the bare default both mean every set — the help text
            // advertises `all`, so it must not fall through to the name lookup and
            // error (Bug 2).
            None | Some("all") => FIXTURES.to_vec(),
            Some(name) => {
                let found = FIXTURES
                    .iter()
                    .find(|(n, _)| *n == name)
                    .copied()
                    .ok_or_else(|| {
                        let names: Vec<&str> = FIXTURES.iter().map(|(n, _)| *n).collect();
                        anyhow::anyhow!(
                            "unknown fixture set {name:?}; choose one of: {}",
                            names.join(", ")
                        )
                    })?;
                vec![found]
            }
        };

        let mut reports: Vec<FixtureReport> = Vec::new();
        for (name, builder) in picked {
            reports
                .push(builder(root, scale).with_context(|| format!("building fixture {name:?}"))?);
        }

        println!("fixture summary:");
        for r in &reports {
            println!(
                "  {:16} {:>5} files, ~{:>10} bytes allocated",
                r.name, r.files, r.alloc
            );
        }
        print_verifications(root);
        Ok(())
    }

    /// Refuse `/`, outside-/tmp roots, and existing non-empty roots unless `--force`.
    fn validate_root(root: &Path, force: bool) -> Result<()> {
        if root.as_os_str().is_empty() {
            bail!("root path is empty");
        }
        if root == Path::new("/") {
            bail!("refusing to build fixtures at '/' — pick a fresh dir under /tmp");
        }
        if force {
            return Ok(());
        }
        let in_tmp = root.starts_with("/tmp")
            && !root.components().any(|c| matches!(c, Component::ParentDir));
        if !in_tmp {
            bail!(
                "refusing to build outside /tmp (got {:?}) — pass --force to override",
                root.display()
            );
        }
        if root.exists() {
            let mut rd =
                std::fs::read_dir(root).with_context(|| format!("read {}", root.display()))?;
            if rd.next().is_some() {
                bail!(
                    "refusing to build into existing non-empty dir {:?} — pass --force to override",
                    root.display()
                );
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ helpers

    fn ensure_dir(p: &Path) -> Result<()> {
        std::fs::create_dir_all(p).with_context(|| format!("mkdir {}", p.display()))
    }

    fn write_str(p: &Path, s: &str) -> Result<()> {
        std::fs::write(p, s.as_bytes()).with_context(|| format!("write {}", p.display()))
    }

    /// Write exactly `size` bytes of deterministic filler (`b'x'`).
    fn write_filler(p: &Path, size: u64) -> Result<()> {
        let mut f = File::create(p).with_context(|| format!("create {}", p.display()))?;
        let buf = vec![FILLER; CHUNK];
        let mut left = size;
        while left > 0 {
            let n = left.min(CHUNK as u64) as usize;
            f.write_all(&buf[..n])
                .with_context(|| format!("write {}", p.display()))?;
            left -= n as u64;
        }
        Ok(())
    }

    /// Set a file's OR directory's mtime (futimens on the opened fd works for both).
    fn set_mtime(p: &Path, t: SystemTime) -> Result<()> {
        let f = File::open(p).with_context(|| format!("open {}", p.display()))?;
        f.set_times(std::fs::FileTimes::new().set_modified(t))
            .with_context(|| format!("set mtime on {}", p.display()))?;
        Ok(())
    }

    /// Set `t` on every file and dir under `root` (symlinks skipped: their mtime rides the
    /// target inode and carries no scan signal).
    fn set_mtime_recursive(root: &Path, t: SystemTime) -> Result<()> {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            set_mtime(&dir, t)?;
            let rd = std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))?;
            for ent in rd.flatten() {
                let p = ent.path();
                let Ok(md) = std::fs::symlink_metadata(&p) else {
                    continue;
                };
                if md.is_dir() {
                    stack.push(p);
                } else if md.is_file() {
                    set_mtime(&p, t)?;
                }
            }
        }
        Ok(())
    }

    fn now_minus(secs: u64) -> SystemTime {
        SystemTime::now()
            .checked_sub(Duration::from_secs(secs))
            .unwrap_or_else(SystemTime::now)
    }

    /// Allocated bytes from `st_blocks * 512` (macOS/Linux). Non-unix falls back to len.
    #[cfg(unix)]
    fn alloc_bytes(md: &std::fs::Metadata) -> u64 {
        use std::os::unix::fs::MetadataExt;
        md.blocks() * 512
    }
    #[cfg(not(unix))]
    fn alloc_bytes(md: &std::fs::Metadata) -> u64 {
        md.len()
    }

    /// Hardlink count of the inode at `p` (0 if unreadable). Non-unix: no nlink concept.
    #[cfg(unix)]
    fn nlink_of(p: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(p).map(|m| m.nlink()).unwrap_or(0)
    }
    #[cfg(not(unix))]
    fn nlink_of(_p: &Path) -> u64 {
        0
    }

    /// Recursive (files, allocated-bytes) count. Symlinks are never descended and never
    /// counted (they carry no reclaimable bytes — mirroring the walker's `Kind::Symlink`).
    fn count_files(root: &Path) -> (u64, u64) {
        let mut files = 0u64;
        let mut alloc = 0u64;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for ent in rd.flatten() {
                let p = ent.path();
                let Ok(md) = std::fs::symlink_metadata(&p) else {
                    continue;
                };
                if md.is_dir() {
                    stack.push(p);
                } else if md.is_file() {
                    files += 1;
                    alloc += alloc_bytes(&md);
                }
            }
        }
        (files, alloc)
    }

    #[cfg(unix)]
    fn make_symlink(target: &str, link: &Path) -> Result<()> {
        std::os::unix::fs::symlink(target, link)
            .with_context(|| format!("symlink {:?} -> {target:?}", link.display()))
    }
    #[cfg(not(unix))]
    fn make_symlink(_target: &str, _link: &Path) -> Result<()> {
        bail!("symlink fixtures require a unix platform")
    }

    // ------------------------------------------------------- macOS flag helpers

    /// `chflags(UF_HIDDEN)` — macOS only. Non-macOS: mode 0700 alone (no hidden flag).
    #[cfg(target_os = "macos")]
    fn set_uf_hidden(p: &Path) -> Result<()> {
        let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes())
            .map_err(|_| anyhow::anyhow!("NUL byte in path {}", p.display()))?;
        // SAFETY: `c` is a valid NUL-terminated C string for the duration of the call;
        // `chflags` mutates only the target inode, not the argument buffer.
        let rc = unsafe { libc::chflags(c.as_ptr(), UF_HIDDEN) };
        if rc != 0 {
            bail!(
                "chflags UF_HIDDEN {}: {}",
                p.display(),
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    fn set_uf_hidden(_p: &Path) -> Result<()> {
        Ok(())
    }

    /// lstat + BSD `st_flags` (same technique as classify's probe). Non-macOS: 0.
    #[cfg(target_os = "macos")]
    fn flags_of(p: &Path) -> u32 {
        let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else {
            return 0;
        };
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: `c` valid NUL-terminated; `st` is a correctly-sized out-buffer.
        let rc = unsafe { libc::lstat(c.as_ptr(), &mut st) };
        if rc != 0 { 0 } else { st.st_flags }
    }
    #[cfg(not(target_os = "macos"))]
    fn flags_of(_p: &Path) -> u32 {
        0
    }

    // --------------------------------------------------------------- fixtures

    /// `demo-app/` — manifest + lockfile (lockfile newer than node_modules dir) + a
    /// ~1 MiB node_modules tree. Classify: `LockfilePresent` ⇒ `RebuildableFromSource`.
    fn build_demo_app(root: &Path, scale: u32) -> Result<FixtureReport> {
        let app = root.join("demo-app");
        let nm = app.join("node_modules");
        ensure_dir(&app)?;

        write_str(
            &app.join("package.json"),
            r#"{"name":"demo-app","version":"0.1.0","private":true,"dependencies":{"left-pad":"^1.3.0","ms":"^2.1.3","@scope/pkg":"^1.0.0"}}"#,
        )?;
        write_str(
            &app.join("package-lock.json"),
            r#"{"name":"demo-app","lockfileVersion":3,"requires":true,"packages":{"":{"name":"demo-app","version":"0.1.0"},"node_modules/left-pad":{"version":"1.3.0"},"node_modules/ms":{"version":"2.1.3"},"node_modules/@scope/pkg":{"version":"1.0.0"}}}"#,
        )?;

        const MODULES: &[(&str, &str)] = &[
            ("left-pad", "index.js"),
            ("ms", "index.js"),
            ("@scope/pkg", "index.js"),
            ("@scope/pkg", "util.js"),
        ];
        for (sub, fname) in MODULES {
            let stem = fname.trim_end_matches(".js");
            let dir = nm.join(sub);
            ensure_dir(&dir)?;
            for i in 0..scale {
                let name = if i == 0 {
                    fname.to_string()
                } else {
                    format!("{stem}-{i}.js")
                };
                write_filler(&dir.join(name), 256 * KIB)?;
            }
        }

        // Cold everywhere; lockfile slightly NEWER than the node_modules dir (SPEC: lockfile
        // newer than dir ⇒ deterministic regen).
        let cold = now_minus(COLD_SECS);
        set_mtime_recursive(&app, cold)?;
        set_mtime(
            &app.join("package-lock.json"),
            cold + Duration::from_secs(5),
        )?;

        let (files, alloc) = count_files(&app);
        Ok(FixtureReport {
            name: "demo-app",
            files,
            alloc,
        })
    }

    /// `orphan-node/` — bare `node_modules/` with NO manifest anywhere above.
    /// Classify: `OrphanTree` ⇒ `Expensive` (prompt-only, never auto-delete).
    fn build_orphan_node(root: &Path, _scale: u32) -> Result<FixtureReport> {
        let top = root.join("orphan-node");
        let nm = top.join("node_modules");
        ensure_dir(&nm.join("left-pad"))?;
        ensure_dir(&nm.join("ms"))?;
        write_filler(&nm.join("left-pad/index.js"), 64 * KIB)?;
        write_filler(&nm.join("ms/index.js"), 64 * KIB)?;
        set_mtime_recursive(&top, now_minus(COLD_SECS))?;
        let (files, alloc) = count_files(&top);
        Ok(FixtureReport {
            name: "orphan-node",
            files,
            alloc,
        })
    }

    /// `.venv/` — orphan venv: `pyvenv.cfg` marker, bin/, site-packages; no
    /// pyproject.toml/requirements.txt above. Classify: `OrphanTree` ⇒ `Expensive`.
    fn build_venv(root: &Path, scale: u32) -> Result<FixtureReport> {
        let v = root.join(".venv");
        let sp = v.join("lib/python3.12/site-packages");
        ensure_dir(&v.join("bin"))?;
        ensure_dir(&sp)?;
        write_str(
            &v.join("pyvenv.cfg"),
            "home = /opt/homebrew/bin\nversion = 3.12.0\nexecutable = /opt/homebrew/bin/python3.12\n",
        )?;
        write_filler(&v.join("bin/activate"), KIB)?;
        write_filler(&v.join("bin/python"), KIB)?;
        for i in 0..8 * scale as usize {
            write_filler(&sp.join(format!("pkg{i:03}.py")), 64 * KIB)?;
        }
        set_mtime_recursive(&v, now_minus(COLD_SECS))?;
        let (files, alloc) = count_files(&v);
        Ok(FixtureReport {
            name: "venv",
            files,
            alloc,
        })
    }

    /// `DerivedData/` — Xcode DerivedData shape: Info.plist + build products `.o` files.
    /// Classify: XcodeDerivedData path class ⇒ `RebuildableFromSource` (regen xcodebuild).
    fn build_derived_data(root: &Path, scale: u32) -> Result<FixtureReport> {
        let dd = root.join("DerivedData");
        let build = dd.join("DemoApp-1A2B3C4D/Build/Products/Debug");
        ensure_dir(&build)?;
        ensure_dir(&dd.join("Logs"))?;
        write_str(
            &dd.join("Info.plist"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>WorkspacePath</key><string>/tmp/fx-demo/DemoApp.xcworkspace</string></dict></plist>\n",
        )?;
        write_filler(&dd.join("Logs/Build.log"), 2 * KIB)?;
        const OBJS: &[&str] = &[
            "AppDelegate",
            "SceneDelegate",
            "ViewController",
            "ContentView",
            "main",
            "Assets",
        ];
        for obj in OBJS {
            for i in 0..scale {
                let name = if i == 0 {
                    format!("{obj}.o")
                } else {
                    format!("{obj}-{i}.o")
                };
                write_filler(&build.join(name), 128 * KIB)?;
            }
        }
        set_mtime_recursive(&dd, now_minus(COLD_SECS))?;
        let (files, alloc) = count_files(&dd);
        Ok(FixtureReport {
            name: "derived-data",
            files,
            alloc,
        })
    }

    /// `_cacache/` — npm CAS store: content-v2/sha512/aa/bb/<64-hex>/data + index-v5 entries.
    /// Classify: CasStore("npm") ⇒ `PureCache` (whole-store action only).
    fn build_cacache(root: &Path, scale: u32) -> Result<FixtureReport> {
        let c = root.join("_cacache");
        let content = c.join("content-v2/sha512/aa/bb");
        let index = c.join("index-v5");
        ensure_dir(&content)?;
        ensure_dir(&index)?;
        let n = 8 * scale as usize; // ≥8 content entries (SPEC §7)
        for i in 0..n {
            // Deterministic 64-hex "hash" (unique per index; shape is what matters).
            let hex = format!("{:064x}", (i as u128 + 1) * 0x9e37_79b9_7f4a_7c15);
            let data_dir = content.join(&hex);
            ensure_dir(&data_dir)?;
            write_filler(&data_dir.join("data"), 256 * KIB)?;
            write_filler(&index.join(&hex), 192)?;
        }
        set_mtime_recursive(&c, now_minus(COLD_SECS))?;
        let (files, alloc) = count_files(&c);
        Ok(FixtureReport {
            name: "cacache",
            files,
            alloc,
        })
    }

    /// `pnpm-store/` — CAS content store (`files/`) with one file hardlinked into a
    /// project's node_modules (`nlink == 2`). Classify: CasStore("pnpm") + `Hardlinked`
    /// byte discount (deleting one of N frees 0).
    fn build_pnpm_store(root: &Path, scale: u32) -> Result<FixtureReport> {
        let store = root.join("pnpm-store");
        let files = store.join("files");
        ensure_dir(&files)?;
        let src = files.join("a");
        write_filler(&src, 64 * KIB)?;
        for i in 1..=2 + scale {
            write_filler(&files.join(format!("f-{i}")), 64 * KIB)?;
        }
        let link = root.join("pnpm-linked/node_modules/foo");
        ensure_dir(&link)?;
        std::fs::hard_link(&src, link.join("a")).with_context(|| {
            format!(
                "hard_link {} -> {}",
                src.display(),
                link.join("a").display()
            )
        })?;
        let cold = now_minus(COLD_SECS);
        set_mtime_recursive(&store, cold)?;
        set_mtime(&link.join("a"), cold)?;
        let nlink = nlink_of(&src);
        if nlink != 2 {
            bail!("pnpm-store hardlink verification failed: nlink={nlink}, expected 2");
        }
        let (files, alloc) = count_files(&store);
        Ok(FixtureReport {
            name: "pnpm-store",
            files,
            alloc,
        })
    }

    /// `sparse.img` — 64 MiB apparent, ~8 KiB allocated (set_len + two 1 KiB writes).
    /// Classify: `Sparse` (apparent ≫ allocated) ⇒ `HandsOff` as a file.
    fn build_sparse(root: &Path, _scale: u32) -> Result<FixtureReport> {
        let p = root.join("sparse.img");
        const APPARENT: u64 = 64 * MIB;
        const TAIL: u64 = KIB;
        let mut f = File::create(&p).with_context(|| format!("create {}", p.display()))?;
        f.set_len(APPARENT)?;
        f.write_all(&[FILLER; 1024])?;
        f.seek(SeekFrom::Start(APPARENT - TAIL))?;
        f.write_all(&[FILLER; 1024])?;
        f.sync_all()?;
        set_mtime(&p, now_minus(COLD_SECS))?;
        let alloc = std::fs::metadata(&p)
            .map(|md| alloc_bytes(&md))
            .unwrap_or(0);
        Ok(FixtureReport {
            name: "sparse",
            files: 1,
            alloc,
        })
    }

    /// `symlink-chain/` — chain `a→b→c→missing` (broken tail) + cycle `loop-x↔loop-y`.
    /// The walker must not descend symlinks or loops (`Kind::Symlink`).
    fn build_symlink_chain(root: &Path, _scale: u32) -> Result<FixtureReport> {
        let sc = root.join("symlink-chain");
        ensure_dir(&sc)?;
        for (link, target) in [("a", "b"), ("b", "c"), ("c", "missing")] {
            make_symlink(target, &sc.join(link))?;
        }
        make_symlink("loop-y", &sc.join("loop-x"))?;
        make_symlink("loop-x", &sc.join("loop-y"))?;
        let (files, alloc) = count_files(&sc);
        Ok(FixtureReport {
            name: "symlink-chain",
            files,
            alloc,
        })
    }

    /// `.Trash-janitor/` — quarantine-root shape for M2 execute tests: mode 0700 +
    /// UF_HIDDEN (macOS). Not yet a classify signal in M1 (mask excludes UF_HIDDEN).
    fn build_trash_janitor(root: &Path, _scale: u32) -> Result<FixtureReport> {
        let tr = root.join(".Trash-janitor");
        ensure_dir(&tr)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&tr)?.permissions();
            perm.set_mode(0o700);
            std::fs::set_permissions(&tr, perm)?;
        }
        set_uf_hidden(&tr)?;
        let (files, alloc) = count_files(&tr);
        Ok(FixtureReport {
            name: "trash-janitor",
            files,
            alloc,
        })
    }

    /// `hot-project/` — manifest + node_modules with ONE file whose mtime is NOW
    /// (dir + everything else cold). Classify: `MtimeHot` ⇒ `HandsOff`.
    fn build_hot_project(root: &Path, _scale: u32) -> Result<FixtureReport> {
        let hp = root.join("hot-project");
        let nm = hp.join("node_modules");
        ensure_dir(&nm)?;
        write_str(
            &hp.join("package.json"),
            r#"{"name":"hot-project","version":"0.1.0","private":true,"dependencies":{"hot-dep":"^1.0.0"}}"#,
        )?;
        write_filler(&nm.join("hot.js"), 64 * KIB)?;
        set_mtime_recursive(&hp, now_minus(COLD_SECS))?;
        // One actively-written file inside the candidate subtree — max_mtime = NOW.
        set_mtime(&nm.join("hot.js"), SystemTime::now())?;
        let (files, alloc) = count_files(&hp);
        Ok(FixtureReport {
            name: "hot-project",
            files,
            alloc,
        })
    }

    // ------------------------------------------------------------ verifications

    /// Runtime checks printed after the summary: hardlink nlink, sparse apparent/alloc,
    /// `.Trash-janitor` st_flags, symlink chain/loop layout + broken tail.
    fn print_verifications(root: &Path) {
        let src = root.join("pnpm-store/files/a");
        let nlink = nlink_of(&src);
        println!(
            "  hardlink check {}: nlink={nlink} {}",
            src.display(),
            if nlink == 2 { "(OK)" } else { "(FAIL)" }
        );

        let sp = root.join("sparse.img");
        match std::fs::metadata(&sp) {
            Ok(md) => println!(
                "  sparse check {}: {} B apparent, ~{} B allocated",
                sp.display(),
                md.len(),
                alloc_bytes(&md)
            ),
            Err(_) => println!("  sparse check: {} missing", sp.display()),
        }

        let tr = root.join(".Trash-janitor");
        let flags = flags_of(&tr);
        println!(
            "  .Trash-janitor st_flags = 0x{flags:08x} ({})",
            if flags & UF_HIDDEN != 0 {
                "UF_HIDDEN set"
            } else {
                "UF_HIDDEN absent"
            }
        );

        let sc = root.join("symlink-chain");
        for (link, want) in [
            ("a", "b"),
            ("b", "c"),
            ("c", "missing"),
            ("loop-x", "loop-y"),
            ("loop-y", "loop-x"),
        ] {
            match std::fs::read_link(sc.join(link)) {
                Ok(target) => {
                    let ok = target == Path::new(want);
                    println!(
                        "  symlink {} -> {} ({})",
                        sc.join(link).display(),
                        target.display(),
                        if ok { "OK" } else { "MISMATCH" }
                    );
                }
                Err(_) => println!("  symlink {} (missing)", sc.join(link).display()),
            }
        }
        let broken = std::fs::symlink_metadata(sc.join("c")).is_ok()
            && std::fs::metadata(sc.join("c")).is_err();
        println!("  symlink-chain: broken tail c->missing = {broken}");
    }
}
