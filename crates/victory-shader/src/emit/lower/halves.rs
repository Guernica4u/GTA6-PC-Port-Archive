//! Console wave64 compute programs on a host whose waves have at most 32
//! lanes (NVIDIA, Intel: `WaveLaneCountMax` 32). Such a host refuses
//! `WaveSize(64)`, which a wave64 program that needs the console's lanes
//! ([`Lower::console_lanes`]) declares, so its pipeline was never made.
//!
//! Each console wave runs as two host waves of 32 lanes ("halves"): the
//! thread relabelling ([`Lower::thread_id`]) gives each host wave a 32-aligned
//! run of flat thread ids, so host wave `2w + h` holds console wave `w`'s
//! lanes `32h..32h + 31`, host lane `n` being console lane `32h + n`. What
//! a lane reads of its own half is exact; reads into the other half have no
//! host form and are refused where they are made:
//!
//! * within a half, exact: DPP (rows of 16, DPP8 groups of 8), `ds_swizzle`
//!   (groups of 32 or quads), and `ds_bpermute_b32`, which in wave64 permutes
//!   each half on its own (RDNA2 ISA, "Data Share Operations"; measured on
//!   RDNA2 in upstream PR #573, banbanzhige's). A lane's own bit of a mask
//!   held as a number (a constant, data loaded or computed by scalar code),
//!   `v_writelane_b32` (the lane named takes the scalar) and the row a DPP
//!   row mask names (rows 2 and 3 are the upper half's) are decided by the
//!   console lane ([`Lower::console_lane`]);
//! * across halves, traded: `v_readlane_b32` ([`Lower::lane_read`]). The
//!   halves store what the other may read in group-shared memory and meet
//!   at a group barrier, which is legal only where every thread of the group
//!   gets there together ([`Exchange`]); elsewhere the read is refused;
//! * across halves, refused: ballots (a mask read as data holds the other
//!   half's lanes) and whole-wave reductions and prefix sums (SGPR spills
//!   read back are not lane reads). In a group of 32 threads or fewer, which one host wave
//!   holds whole, nothing is in the other half: its ballots' high dwords are
//!   0 and its reductions run over every lane the console has, as on the
//!   console, and a lane read of a constant lane below 32 is the host's;
//! * `v_readfirstlane_b32` reads the host wave's first lane, which in the
//!   upper half is console lane 32 or later: kept for a value the same in
//!   every lane of the console wave ([`lane_uniform`], what the compiler
//!   reads first lanes for) and in a group of 32 threads or fewer (one host
//!   wave holds the whole console wave), refused otherwise ([`FIRST_ACROSS`]).
//!
//! A refusal in a value no instruction ends up needing costs nothing (the
//! lowering retries with that value missing), so a program is refused only
//! when it really reads across halves.
//!
//! Thread ids: the relabelling takes each host wave to hold a 32-aligned run
//! of flat ids. Hosts fill the waves of a group of several rows with tiles
//! when its rows are 32 threads wide or more (or not a power of two wide), so
//! such groups are refused ([`Lower::halves_tiles`]).
//!
//! LDS: the console compiler leaves out barriers between the lanes of one
//! wave, and so between all of a group's threads only when the group is one
//! wave. In a group of 33 to 64 threads the two host waves share what the
//! console's one wave ordered by running one instruction at a time, so group
//! barriers put them back in that order ([`Lds`]): before each LDS access
//! that may meet an earlier one where every thread meets, and on the edges
//! into and out of a region only some lanes run that accesses LDS. Inside
//! such a region nothing can be put in order, so one that both writes and
//! reads, or writes twice, is refused. Groups of several console waves
//! order their LDS with barriers, and those of 32 threads or fewer are one
//! host wave.
//!
//! Approach after DexNom's fork (DexNom/Victory 5f4c3df, `set_host_max_wave`
//! and the thread relabelling into halves) and upstream PR #2176
//! (n-romanovich: the setting keyed in the shader cache); here the setting
//! is a translation option ([`super::super::Options::host_wave32`]), and
//! cross-half reads are refused where they are lowered instead of by
//! instruction name. Which values are the same in every lane of the console
//! wave ([`lane_uniform`]): justrealyme's analysis (chippy01/VIctory#1),
//! ported from fxceless/victory-fork (#497 there).

use super::*;

/// Console lanes per host wave in halves.
const HALF: u32 = 32;

impl Lower<'_> {
    /// Whether a wave64 program whose lanes follow the console's runs in
    /// halves: the host's waves are 32 lanes at most.
    pub(super) fn halves_wanted(x: &Parts, needs_wave: bool) -> bool {
        needs_wave && x.native.wave64 && x.host_wave32
    }

    /// Why a program in halves cannot run: a group of several rows whose
    /// rows are 32 threads wide or more, or not a power of two wide. A host
    /// may then fill a wave with a tile of the group (16 x 2 threads) rather
    /// than a run of flat ids, and the relabelling ([`Lower::relabelled_flat`])
    /// would give two threads one id. Measured in halves on an RTX 3070 Ti and
    /// the Radeon iGPU of a Ryzen 9 7900X: 32 x 2, 32 x 8, 64 x 4, 32 x 32 and
    /// 32 x 2 x 2 groups store half their ids; 8 x 4, 8 x 8, 16 x 2, 16 x 4,
    /// 16 x 16, 8 x 16, 4 x 4 x 4, 2 x 2 x 16 and 8 x 8 x 4 all of them (upstream
    /// PR #2126 measured the same on NVIDIA, and widths that are not powers of
    /// two, and hands each wave its ids by ticket instead).
    pub(super) fn halves_tiles([x, y, z]: [u32; 3]) -> Option<Why> {
        (y * z > 1 && (x >= HALF || !x.is_power_of_two()))
            .then_some("a group whose rows a 32-lane host wave may tile (wave64 on a 32-lane host)")
    }

    /// The flat thread id, relabelled so that each host wave holds a run of
    /// [`Shader::wave_size`]-aligned ids from its first lane (see
    /// [`Lower::thread_id`]); made once, in the entry block.
    pub(super) fn relabelled_flat(&mut self) -> V {
        if let Some(f) = self.relabelled {
            return f;
        }
        let e = self.s.entry;
        let i32t = self.s.t.i32;
        let flat = self.s.op(
            e,
            "flattenedThreadIdInGroup.i32",
            Attr::ReadNone,
            i32t,
            96,
            &[],
        );
        let first = self.s.op(
            e,
            "waveReadLaneFirst.i32",
            Attr::NoUnwind,
            i32t,
            118,
            &[flat],
        );
        let wave = self.s.wave_size.expect("set with console lanes");
        let mask = self.s.m.i32(!(wave - 1));
        let base = self.s.m.bin(e, Bin::And, first, mask);
        let lane = self
            .s
            .op(e, "waveGetLaneIndex", Attr::ReadOnly, i32t, 111, &[]);
        let f = self.s.m.bin(e, Bin::Add, base, lane);
        self.s.flags |= flags::WAVE_OPS;
        self.relabelled = Some(f);
        f
    }

    /// This lane's index in its console wave: the host lane, or in halves
    /// the host lane in the half the host wave holds.
    pub(super) fn console_lane(&mut self) -> V {
        if !self.halves {
            return self.lane_index();
        }
        let f = self.relabelled_flat();
        let m = self.s.m.i32(self.wave() - 1);
        self.s.m.bin(self.s.entry, Bin::And, f, m)
    }

    /// Whether the group fits one host wave: a console wave64 of 32 threads
    /// or fewer has no lane in the upper half.
    pub(super) fn one_host_wave(&self) -> bool {
        self.s
            .threads
            .is_some_and(|t| t.iter().product::<u32>() <= HALF)
    }

    /// Refuses a read into the other half of the console wave in halves,
    /// unless the group fits one host wave (no lane in the other half).
    pub(super) fn same_half(&self, what: Why) -> Result<(), Why> {
        if self.halves && !self.one_host_wave() {
            Err(what)
        } else {
            Ok(())
        }
    }

    /// `v_readlane_b32` (`inst`) of `x` at `lane` in halves: `None` where the
    /// host wave's own read is the console's (not halves; a constant lane
    /// below 32 of a group one host wave holds), the lane's own `x` where `x`
    /// is the same in every lane of the console wave ([`lane_uniform`]), else
    /// what the halves trade ([`Lower::across`]). In a group one host wave
    /// holds, any other lane read is refused: a lane past the group holds
    /// whatever its registers did on the console.
    pub(super) fn lane_read(&mut self, inst: &Inst, x: V, lane: V) -> Result<Option<V>, Why> {
        let Fields::Valu(v) = &inst.fields else {
            return Ok(None);
        };
        if !self.halves {
            return Ok(None);
        }
        if self.one_host_wave() {
            let low = v
                .src
                .get(1)
                .and_then(|&l| crate::resource::lane_of(l))
                .is_some_and(|l| l < HALF);
            return if low {
                Ok(None)
            } else {
                Err("a lane read that may cross 32-lane halves (wave64 on a 32-lane host)")
            };
        }
        if let Some(&Operand::Vector { reg, .. }) = v.src.first()
            && inst
                .read(Reg::V(reg))
                .is_some_and(|r| self.alike.contains(&r))
        {
            return Ok(Some(x));
        }
        self.across(x, lane).map(Some)
    }

    /// Lane `lane`'s `x` in every lane of the console wave, traded through
    /// group-shared memory: the first lane of each host wave stores its lane
    /// `lane % 32`'s `x` in its half's slot; after a group barrier every lane
    /// loads the slot of the half holding `lane`, and a second barrier keeps
    /// the next trade from overwriting it first. Only where every thread of
    /// the group meets ([`Exchange`]): a barrier one half skips hangs the GPU
    /// (D3D12 leaves it undefined; NVIDIA waits for every thread).
    fn across(&mut self, x: V, lane: V) -> Result<V, Why> {
        if !self
            .exchange
            .meet
            .get(self.cur_ir)
            .copied()
            .unwrap_or(false)
        {
            return Err(APART);
        }
        let slots = self.slots()?;
        let f = self.relabelled_flat();
        let k = |l: &mut Self, v: u32| l.s.m.i32(v);
        let (k1, k5, k6, k31) = (k(self, 1), k(self, 5), k(self, 6), k(self, 31));
        let c = self.cur;
        let low = self.s.m.bin(c, Bin::And, lane, k31);
        let mine = self.read_lane(x, low);
        let own = self.s.m.bin(c, Bin::LShr, f, k5);
        let i1 = self.s.t.i1;
        let first = self
            .s
            .op(c, "waveIsFirstLane", Attr::NoUnwind, i1, 110, &[]);
        let at = self.begin_guarded(Some(first));
        let p = self.s.m.element(self.cur, slots, own);
        self.s.m.store(self.cur, p, mine);
        self.end_guarded(at, &[]);
        self.group_barrier();
        let c = self.cur;
        let wave = self.s.m.bin(c, Bin::LShr, f, k6);
        let wave = self.s.m.bin(c, Bin::Shl, wave, k1);
        let half = self.s.m.bin(c, Bin::LShr, lane, k5);
        let half = self.s.m.bin(c, Bin::And, half, k1);
        let slot = self.s.m.bin(c, Bin::Or, wave, half);
        let p = self.s.m.element(c, slots, slot);
        let r = self.s.m.load(c, p);
        self.group_barrier();
        Ok(r)
    }

    /// At the start of IR block `b`, after its phis: the group barrier where a
    /// region accessing LDS comes back ([`Lds`]). What LDS accesses are
    /// pending at a block's start is not known.
    pub(super) fn lds_block_start(&mut self, b: usize) {
        let Some(lds) = &self.exchange.lds else {
            return;
        };
        if lds.at_entry.contains(&b) {
            self.group_barrier();
            self.exchange.pending = None;
        } else {
            self.exchange.pending = Some(true);
        }
    }

    /// At the end of IR block `b`, before it branches: the group barrier
    /// before a region accessing LDS ([`Lds`]).
    pub(super) fn lds_block_end(&mut self, b: usize) {
        if self
            .exchange
            .lds
            .as_ref()
            .is_some_and(|l| l.before_exit.contains(&b))
        {
            self.group_barrier();
        }
    }

    /// Before the LDS access `name` where every thread meets: a group
    /// barrier when an access since the last one may meet it (a write, or a
    /// read before this write), as the console's one wave orders them.
    pub(super) fn lds_order(&mut self, name: &str) {
        let meets = self
            .exchange
            .meet
            .get(self.cur_ir)
            .copied()
            .unwrap_or(false);
        if self.exchange.lds.is_none() || !meets {
            return;
        }
        let writes = !name.starts_with("ds_read");
        let pending = self.exchange.pending;
        if pending == Some(true) || writes && pending == Some(false) {
            self.group_barrier();
            self.exchange.pending = None;
        }
        self.exchange.pending = Some(writes || self.exchange.pending == Some(true));
    }

    /// A group barrier ordering group-shared memory
    /// (`GroupMemoryBarrierWithGroupSync`).
    fn group_barrier(&mut self) {
        let mode = self.s.m.i32(1 | 8);
        let void = self.s.t.void;
        self.s
            .op(self.cur, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
    }

    /// The trade's group-shared slots, made once: a dword per half of each
    /// console wave of the group, beside the program's own LDS and within
    /// the host's 32 KiB.
    fn slots(&mut self) -> Result<V, Why> {
        if let Some(g) = self.exchange.slots {
            return Ok(g);
        }
        let group = self.s.threads.map_or(64, |t| t.iter().product::<u32>());
        let dwords = 2 * group.div_ceil(64);
        if self.s.lds_dwords + dwords > 8192 {
            return Err(
                "a trade between 32-lane halves past the host's 32 KiB of group-shared memory",
            );
        }
        let g = self.s.shared("halves", dwords);
        self.exchange.slots = Some(g);
        Ok(g)
    }
}

/// Why a lane read stops in halves: the halves cannot meet there.
pub(super) const APART: Why = "a lane read across 32-lane halves where not every thread of the group meets (wave64 on a 32-lane host)";

/// Trades between the two halves of each console wave: where they may
/// meet, and the group-shared slots they trade through.
#[derive(Default)]
pub(super) struct Exchange {
    /// Per IR block: every thread of the group gets there together, the
    /// same number of times ([`meeting`]).
    meet: Vec<bool>,
    /// [`Lower::slots`], once made.
    slots: Option<V>,
    /// A group of one console wave with LDS: where it needs barriers.
    lds: Option<Lds>,
    /// Why the program cannot run in halves ([`lds_fences`]).
    pub(super) refused: Option<Why>,
    /// LDS accesses since the last barrier in this block where every thread
    /// meets: `Some(true)` with a write among them (or not known, at a
    /// block's start), `Some(false)` reads only.
    pending: Option<bool>,
}

impl Exchange {
    pub(super) fn new(x: &Parts, halves: bool) -> Self {
        if !halves {
            return Exchange::default();
        }
        let meet = meeting(x);
        let one_wave_lds = match &x.native.io {
            StageIo::Cs(cs) => {
                let group = cs.threads.iter().product::<u32>();
                cs.lds_bytes() > 0 && group > HALF && group <= 64
            }
            _ => false,
        };
        let (lds, refused) = match one_wave_lds.then(|| lds_fences(x, &meet)) {
            Some(Ok(l)) => (Some(l), None),
            Some(Err(why)) => (None, Some(why)),
            None => (None, None),
        };
        Exchange {
            meet,
            slots: None,
            lds,
            refused,
            pending: None,
        }
    }
}

/// Where a group of one console wave orders its LDS in halves: the blocks
/// ending with a group barrier (before they branch into a region only some
/// lanes run that accesses LDS) and those starting with one (where such a
/// region comes back).
#[derive(Default)]
struct Lds {
    before_exit: HashSet<usize>,
    at_entry: HashSet<usize>,
}

/// [`Lds`] of a program, or why it cannot run in halves: a region only some
/// lanes run (a set of blocks not every thread meets in, joined by edges)
/// that both writes and reads LDS, or writes it in two instructions, cannot
/// be put in the console's order without a barrier inside it.
fn lds_fences(x: &Parts, meet: &[bool]) -> Result<Lds, Why> {
    const MIXED: Why = "LDS a branch only some lanes take both writes and reads (or writes twice), in a group of one console wave64 on a host of 32-lane waves";
    let (p, st) = (x.p, x.st);
    let n = st.exits.len();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, e) in st.exits.iter().enumerate() {
        match *e {
            LaneExit::Jump(t) => succ[b].push(t),
            LaneExit::Branch {
                taken, not_taken, ..
            } => {
                succ[b].push(taken);
                succ[b].push(not_taken);
            }
            LaneExit::End => {}
            LaneExit::Indirect => return Err(MIXED),
        }
    }
    let meets = |b: usize| meet.get(b).copied().unwrap_or(false);
    // The regions: blocks not every thread meets in, by their edges.
    let mut root: Vec<usize> = (0..n).collect();
    fn find(root: &mut [usize], mut b: usize) -> usize {
        while root[b] != b {
            root[b] = root[root[b]];
            b = root[b];
        }
        b
    }
    for (b, next) in succ.iter().enumerate() {
        for &t in next {
            if !meets(b) && !meets(t) {
                let (rb, rt) = (find(&mut root, b), find(&mut root, t));
                root[rb] = rt;
            }
        }
    }
    // Each region's LDS reads, writes and other accesses (atomics).
    let mut access: HashMap<usize, [u32; 3]> = HashMap::new();
    for (b, block) in p.blocks.iter().enumerate() {
        if meets(b) {
            continue;
        }
        for inst in &block.insts {
            let name = inst.name();
            if !matches!(inst.fields, Fields::Ds { .. })
                || name.starts_with("ds_swizzle")
                || name.contains("permute")
                || name == "ds_nop"
            {
                continue;
            }
            let k = if name.starts_with("ds_read") {
                0
            } else if name.starts_with("ds_write") {
                1
            } else {
                2
            };
            access.entry(find(&mut root, b)).or_default()[k] += 1;
        }
    }
    if access
        .values()
        .any(|&[r, w, a]| (w > 0 && r > 0) || (a > 0 && w + r > 0) || a > 1 || w > 1)
    {
        return Err(MIXED);
    }
    let mut lds = Lds::default();
    for (u, next) in succ.iter().enumerate() {
        for &v in next {
            if meets(u) && !meets(v) && access.contains_key(&find(&mut root, v)) {
                lds.before_exit.insert(u);
            }
            if !meets(u) && meets(v) && access.contains_key(&find(&mut root, u)) {
                lds.at_entry.insert(v);
            }
        }
    }
    Ok(lds)
}

/// Per IR block, whether every thread of the group gets there together and
/// the same number of times, so that a group barrier there is legal: no
/// branch on a lane-dependent condition has it in its region (the per-lane
/// form branches on the console's wave-wide tests too). In a group of
/// several console waves, which may each take a uniform branch their own
/// way, no branch at all. Nowhere with an indirect exit.
fn meeting(x: &Parts) -> Vec<bool> {
    let st = x.st;
    let several = match &x.native.io {
        StageIo::Cs(cs) => cs.threads.iter().product::<u32>() > 64,
        _ => true,
    };
    let Some(regions) = branch_regions(st, several) else {
        return vec![false; st.exits.len()];
    };
    let mut meet = vec![true; st.exits.len()];
    for (_, region) in regions {
        for (b, r) in region.iter().enumerate() {
            if *r {
                meet[b] = false;
            }
        }
    }
    meet
}

/// Why a first-lane read stops in halves: the console's first lane may be
/// in the other half, and the value is not known to be the same there.
pub(super) const FIRST_ACROSS: Why = "a first-lane read of a value that may differ between 32-lane halves (wave64 on a 32-lane host)";

/// Every per-lane branch (by its block) with its region: the blocks
/// reachable from it that do not post-dominate it, which only some lanes
/// run. Branches on uniform conditions too with `uniform`. None with an
/// indirect exit.
fn branch_regions(st: &Structured, uniform: bool) -> Option<Vec<(usize, Vec<bool>)>> {
    let n = st.exits.len();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, e) in st.exits.iter().enumerate() {
        match *e {
            LaneExit::Jump(t) => succ[b].push(t),
            LaneExit::Branch {
                taken, not_taken, ..
            } => {
                succ[b].push(taken);
                succ[b].push(not_taken);
            }
            LaneExit::End => {}
            LaneExit::Indirect => return None,
        }
    }
    // Post-dominators, as bitsets (a virtual exit after every end).
    let words = n.div_ceil(64);
    let full = vec![u64::MAX; words];
    let mut pdom: Vec<Vec<u64>> = (0..n)
        .map(|b| {
            if succ[b].is_empty() {
                let mut v = vec![0; words];
                v[b / 64] |= 1 << (b % 64);
                v
            } else {
                full.clone()
            }
        })
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for b in (0..n).rev() {
            if succ[b].is_empty() {
                continue;
            }
            let mut v = full.clone();
            for &t in &succ[b] {
                for (w, p) in v.iter_mut().zip(&pdom[t]) {
                    *w &= p;
                }
            }
            v[b / 64] |= 1 << (b % 64);
            if v != pdom[b] {
                pdom[b] = v;
                changed = true;
            }
        }
    }
    let mut regions = Vec::new();
    for (d, e) in st.exits.iter().enumerate() {
        let LaneExit::Branch { when, .. } = *e else {
            continue;
        };
        if !uniform
            && st
                .bools
                .support(when)
                .iter()
                .all(|a| matches!(a, Atom::Uniform(_)))
        {
            continue;
        }
        let mut seen = vec![false; n];
        let mut stack: Vec<usize> = succ[d].clone();
        while let Some(b) = stack.pop() {
            if seen[b] || pdom[d][b / 64] >> (b % 64) & 1 == 1 {
                continue;
            }
            seen[b] = true;
            stack.extend(succ[b].iter().copied());
        }
        regions.push((d, seen));
    }
    Some(regions)
}

/// The thread id's x (`v0` on entry) of a wave64 compute program whose
/// console waves are runs of 64 ids along x (a group 64-aligned along x,
/// or one row): `id >> 6` and up is one value per console wave.
fn wave_tid(x: &Parts) -> Option<Value> {
    let StageIo::Cs(cs) = &x.native.io else {
        return None;
    };
    let [tx, ty, tz] = cs.threads;
    if !x.native.wave64 || !(tx % 64 == 0 || ty * tz == 1) {
        return None;
    }
    x.p.inputs
        .iter()
        .find(|(r, _)| *r == Reg::V(0))
        .map(|(_, v)| *v)
}

/// VGPR values that are the same in every lane of a console wave holding
/// them, so that a first-lane read in either half gives the console's
/// value: written lane by lane from scalars, constants and such values, by
/// an instruction every arriving lane runs or that runs under a condition
/// itself the same in every lane (then only over an old value that is such
/// a value too); loaded from LDS, or from a buffer through a scalar
/// descriptor, at such an address; or the thread id's x shifted right past
/// the console wave ([`wave_tid`]). The console compiler keeps wave-uniform
/// data in VGPRs and fetches it with `v_readfirstlane` (a typed buffer's
/// element, say, which only a vector load reads). A scalar is one value,
/// except the dwords a waterfall loop reads, which are each lane's own.
///
/// A phi counts where every predecessor is a block every lane of the wave
/// reaches together: no branch on a lane-dependent condition can have split
/// them (a branch on alike values does not). The analysis runs to a fixed
/// point from nothing, so a loop-carried value that needs itself to be
/// alike never is.
pub(super) fn lane_uniform(x: &Parts) -> HashSet<Value> {
    let (p, st) = (x.p, x.st);
    let wave_tid = wave_tid(x);
    let regions = branch_regions(st, false);
    let is_mask = |v: Value| st.masks.contains_key(&v) || st.highs.contains_key(&v);
    let per_lane = |v: Value| x.res.waterfall.reads.contains(&v);
    let mut same: HashSet<Value> = HashSet::new();
    loop {
        let alike_atom = |same: &HashSet<Value>, a: &Atom| match *a {
            Atom::Uniform(_) => true,
            Atom::Lane(v) => same.contains(&v),
            _ => false,
        };
        let alike_bool = |same: &HashSet<Value>, b: Bool| {
            st.bools.support(b).iter().all(|a| alike_atom(same, a))
        };
        // Blocks every lane of a console wave reaches together.
        let wave_all: Vec<bool> = match &regions {
            Some(regions) => {
                let mut w = vec![true; st.exits.len()];
                for (d, region) in regions {
                    let LaneExit::Branch { when, .. } = st.exits[*d] else {
                        continue;
                    };
                    if alike_bool(&same, when) {
                        continue;
                    }
                    for (b, r) in region.iter().enumerate() {
                        if *r {
                            w[b] = false;
                        }
                    }
                }
                w
            }
            None => vec![false; st.exits.len()],
        };
        let mut next: HashSet<Value> = HashSet::new();
        for b in crate::ir::reverse_postorder(&p.blocks) {
            let block = &p.blocks[b];
            let whole = block
                .preds
                .iter()
                .all(|&q| wave_all.get(q).copied().unwrap_or(false));
            for phi in &block.phis {
                if matches!(phi.reg, Reg::V(_))
                    && whole
                    && phi
                        .args
                        .iter()
                        .all(|a| next.contains(a) || same.contains(a))
                {
                    next.insert(phi.value);
                }
            }
            let known = |next: &HashSet<Value>, v: &Value| next.contains(v) || same.contains(v);
            for (i, inst) in block.insts.iter().enumerate() {
                let name = inst.name();
                // Under a condition: only one every lane holds alike, and
                // over an old value that is alike too.
                if let Some(g) = st.guards[b][i] {
                    if !alike_bool(&same, g) {
                        continue;
                    }
                    let old_alike = inst
                        .writes
                        .iter()
                        .filter(|w| matches!(w.0, Reg::V(_)))
                        .all(|w| inst.read(w.0).is_none_or(|old| known(&next, &old)));
                    if !old_alike {
                        continue;
                    }
                }
                // A buffer load at an alike index and offset, through a scalar
                // descriptor, reads one value in every lane.
                if let Fields::Buffer {
                    addr,
                    rsrc,
                    soffset,
                    offen,
                    idxen,
                    ..
                } = &inst.fields
                    && (name.starts_with("buffer_load") || name.starts_with("tbuffer_load"))
                {
                    let vector = |op: &Operand| match *op {
                        Operand::Vector { reg, dwords } => (0..dwords)
                            .all(|k| inst.read(Reg::V(reg + k)).is_some_and(|v| known(&next, &v))),
                        _ => true,
                    };
                    let scalar = |op: &Operand| match *op {
                        Operand::Scalar { code, dwords } if code != NULL => (0..dwords)
                            .all(|k| inst.read(Reg::S(code + k)).is_none_or(|v| !per_lane(v))),
                        _ => true,
                    };
                    if (!(*offen || *idxen) || vector(addr)) && scalar(rsrc) && scalar(soffset) {
                        next.extend(
                            inst.writes
                                .iter()
                                .filter(|w| matches!(w.0, Reg::V(_)))
                                .map(|w| w.1),
                        );
                    }
                    continue;
                }
                // An LDS read at an alike address reads one value in every lane.
                if let Fields::Ds {
                    addr: Some(Operand::Vector { reg, .. }),
                    dst: Some(_),
                    data,
                    ..
                } = &inst.fields
                    && name.starts_with("ds_read")
                    && data.is_empty()
                {
                    if inst.read(Reg::V(*reg)).is_some_and(|a| known(&next, &a)) {
                        next.extend(
                            inst.writes
                                .iter()
                                .filter(|w| matches!(w.0, Reg::V(_)))
                                .map(|w| w.1),
                        );
                    }
                    continue;
                }
                let ops: Vec<Operand> = match &inst.fields {
                    Fields::Valu(v)
                        if v.dpp.is_none()
                            && v.sdwa.is_none()
                            && !name.starts_with("v_mbcnt")
                            && !name.starts_with("v_cmpx")
                            && !name.contains("lane") =>
                    {
                        v.src.clone()
                    }
                    _ => continue,
                };
                // The thread id's x shifted past the console wave: one value
                // per console wave (the b32 shift uses five bits of its count).
                if let (
                    Some(tid),
                    "v_lshrrev_b32",
                    [
                        Operand::Const(k @ 128..=192),
                        Operand::Vector { reg, .. },
                        ..,
                    ],
                ) = (wave_tid, name, ops.as_slice())
                    && ((k - 128) & 31) >= 6
                    && inst.read(Reg::V(*reg)) == Some(tid)
                {
                    next.extend(inst.writes.iter().filter(|w| w.0 != Reg::Scc).map(|w| w.1));
                    continue;
                }
                let alike = ops.iter().all(|op| match *op {
                    Operand::Vector { reg, .. } => {
                        // every dword the instruction reads from that register on
                        let read: Vec<Value> = inst
                            .reads
                            .iter()
                            .filter(|r| matches!(r.0, Reg::V(x) if x >= reg))
                            .map(|r| r.1)
                            .collect();
                        !read.is_empty() && read.iter().all(|x| known(&next, x))
                    }
                    // A scalar is one value; a lane mask is not, unless a
                    // compare of alike values made it.
                    Operand::Scalar { code, .. } if code != NULL => inst
                        .read(Reg::S(code))
                        .is_none_or(|x| !is_mask(x) && !per_lane(x) || known(&next, &x)),
                    _ => true,
                });
                if alike {
                    next.extend(inst.writes.iter().filter(|w| w.0 != Reg::Scc).map(|w| w.1));
                }
            }
        }
        if next.is_subset(&same) {
            return same;
        }
        same.extend(next);
    }
}
