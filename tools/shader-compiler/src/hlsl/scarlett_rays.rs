//! Lower ProjectScarlett's inlined console ray walks to native ray queries.
//! The title compiles each query into a loop over raw console BVH nodes. The
//! loop is replaced by one native query against the scene the runtime binds
//! in its private space; the code around it keeps the guest ISA.
use super::{packed_root::matches_program, Emitter, Enc, Insn, Opnd, ShaderExtract, Stage};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Walk {
    /// CS_IrradianceCache_ShadowTrace: one occlusion ray per cache texel.
    IrradianceShadow,
    /// CS_IrradianceCache_ShadowTrace_WithFar: the same ray against the near
    /// scene, then, if nothing was hit, its remainder against the far scene.
    IrradianceShadowFar,
    /// CS_RaytraceShadows_Mixed: a sun shadow ray per pixel against the near
    /// scene and, if nothing was hit, against the far scene.
    RaytraceShadows,
    /// CS_LaserLightShadowRaytracing: per lamp, the distance to the nearest
    /// triangle along its beam.
    LaserLightShadow,
}

pub(crate) fn identify(ex: &ShaderExtract) -> Option<Walk> {
    if ex.stage != Stage::Compute || ex.psv_threads != Some([32, 1, 1]) {
        return None;
    }
    match ex.entry.as_deref()? {
        "CS_IrradianceCache_ShadowTrace_ad5ad774_Wrapped"
            if matches_program(&ex.isa, 2832, 0x43fba5aebffdef56) =>
        {
            Some(Walk::IrradianceShadow)
        }
        "CS_IrradianceCache_ShadowTrace_WithFar_ad5ad774_Wrapped"
            if matches_program(&ex.isa, 4992, 0x41caa1a510538f9d) =>
        {
            Some(Walk::IrradianceShadowFar)
        }
        "CS_RaytraceShadows_Mixed_22f3acbc_Wrapped"
            if matches_program(&ex.isa, 7728, 0x49e8da1f58f8e753) =>
        {
            Some(Walk::RaytraceShadows)
        }
        "CS_LaserLightShadowRaytracing_c3ab3042_Wrapped"
            if matches_program(&ex.isa, 3332, 0xab98828801484e03) =>
        {
            Some(Walk::LaserLightShadow)
        }
        _ => None,
    }
}

pub(super) const MNEMONIC: &str = "tb_scarlett_ray_query";

/// One walk in the guest ISA.
struct Loop {
    /// Instructions before the walk that only feed it: the load of the scene's
    /// root node pointer, the store that seeds the shared-memory stack, and
    /// the setup of lane storage the walk spills to.
    scratch: &'static [usize],
    /// From the walk's first instruction through its back edge. Nothing
    /// outside this range branches into it except to `at`, and nothing in it
    /// branches out.
    first: usize,
    last: usize,
    /// Where the query goes: the instruction after the entry branch, or the
    /// target of a branch from elsewhere.
    at: usize,
    /// World-space ray: origin xyz, direction xyz, maximum distance. The walk
    /// copies these before it starts, so they hold the world ray throughout.
    ray: [u8; 7],
    /// Register holding the minimum distance, if the walk rejects hits closer
    /// than one; otherwise the walk accepts t >= 0.
    min: Option<u8>,
    /// Nonzero after an accepted triangle; zero before the walk.
    hit: u8,
    /// The walk keeps the nearest triangle: each accepted one shortens the
    /// maximum distance (`ray[6]`) to its own, so after the walk that register
    /// holds the nearest hit. Otherwise the walk stops at the first triangle.
    closest: bool,
    /// A scalar load of the scene's root node that code outside the walk also
    /// tests (zero for an empty scene). It becomes a nonzero constant; the
    /// native query handles an empty scene itself.
    root: Option<(usize, u8)>,
    /// The scene's descriptor: the user SGPR holding the table pointer (the
    /// walk reads it through a copy) and the byte offset in the table.
    table: u8,
    scene: u32,
}

const IRRADIANCE_SHADOW: &[Loop] = &[Loop {
    scratch: &[0x017c, 0x0334],
    first: 0x03bc,
    last: 0x0acc,
    at: 0x03c0,
    ray: [9, 7, 8, 17, 5, 45, 12],
    min: None,
    hit: 10,
    table: 5,
    scene: 128,
    closest: false,
    root: None,
}];

// The near walk keeps its world ray in v45, v2, v4 (origin), v12, v11, v10
// (direction) and v14 (distance). The far walk starts where the near ray
// leaves a sphere around the cache and runs only if the near walk missed.
const IRRADIANCE_SHADOW_FAR: &[Loop] = &[
    Loop {
        scratch: &[0x0384, 0x0418],
        first: 0x0428,
        last: 0x0b34,
        at: 0x042c,
        ray: [45, 2, 4, 12, 11, 10, 14],
        min: None,
        hit: 7,
        table: 5,
        scene: 128,
        closest: false,
        root: None,
    },
    Loop {
        scratch: &[0x0bc4, 0x0b7c],
        first: 0x0c24,
        last: 0x133c,
        at: 0x0c28,
        ray: [14, 15, 9, 17, 18, 13, 46],
        min: None,
        hit: 7,
        table: 5,
        scene: 96,
        closest: false,
        root: None,
    },
];

// Every walk traces origin v4, v5, v6 and direction v9, v10, v11 between v7
// and v8. The far scene is traced by one of two copies of the walk; the copy
// at 0x1658 is entered by the branch at 0x0e8c, which jumps to 0x1660.
const RAYTRACE_SHADOWS: &[Loop] = &[
    Loop {
        scratch: &[0x06b0, 0x0678],
        first: 0x06d4,
        last: 0x0ddc,
        at: 0x06d8,
        ray: [4, 5, 6, 9, 10, 11, 8],
        min: Some(7),
        hit: 15,
        table: 5,
        scene: 128,
        closest: false,
        root: None,
    },
    Loop {
        scratch: &[0x0ec0, 0x0f14],
        first: 0x0f24,
        last: 0x162c,
        at: 0x0f28,
        ray: [4, 5, 6, 9, 10, 11, 8],
        min: Some(7),
        hit: 14,
        table: 5,
        scene: 32,
        closest: false,
        root: None,
    },
    Loop {
        scratch: &[0x0e30, 0x0e7c],
        first: 0x1658,
        last: 0x1d6c,
        at: 0x1660,
        ray: [4, 5, 6, 9, 10, 11, 8],
        min: Some(7),
        hit: 14,
        table: 5,
        scene: 32,
        closest: false,
        root: None,
    },
];

// The walk runs from 0x0170 to its last back edge at 0x0cbc. Lanes 12-14 of
// v5, cleared at 0x008c-0x00b4, are its spill area. The branch at
// 0x00ec skips it when the scene's root word (s32, loaded at 0x00c4) is not
// positive. The beam starts at v1, v2, v3 with direction v6, v9, v11 and
// length v4; after the walk v4 is the nearest hit and v7 nonzero on a hit.
const LASER_LIGHT_SHADOW: &[Loop] = &[Loop {
    scratch: &[0x008c, 0x0090, 0x0098, 0x00b4],
    first: 0x0170,
    last: 0x0cbc,
    at: 0x0170,
    ray: [1, 2, 3, 6, 9, 11, 4],
    min: None,
    hit: 7,
    table: 5,
    scene: 32,
    closest: true,
    root: Some((0x00c4, 32)),
}];

fn loops(walk: Walk) -> &'static [Loop] {
    match walk {
        Walk::IrradianceShadow => IRRADIANCE_SHADOW,
        Walk::IrradianceShadowFar => IRRADIANCE_SHADOW_FAR,
        Walk::RaytraceShadows => RAYTRACE_SHADOWS,
        Walk::LaserLightShadow => LASER_LIGHT_SHADOW,
    }
}

/// Replaces each walk with a single query instruction. The query names the
/// scene by its descriptor-table slot; the emitter resolves the slot to the
/// title's binding.
pub(super) fn prepare(walk: Walk, ops: &mut Vec<Insn>) {
    for l in loops(walk) {
        let at = l.at;
        ops.retain(|op| {
            op.offset == at
                || !(l.scratch.contains(&op.offset) || (op.offset >= l.first && op.offset <= l.last))
        });
        if let Some((offset, sgpr)) = l.root {
            let load = ops.iter_mut().find(|op| op.offset == offset).expect("matched walk ISA");
            load.mnemonic = "s_mov_b32".into();
            load.enc = Enc::Sop1;
            load.op = 3;
            load.dst = vec![Opnd::Sgpr(sgpr)];
            load.src = vec![Opnd::ConstInt(1)];
        }
        let query = ops.iter_mut().find(|op| op.offset == at).expect("matched walk ISA");
        query.mnemonic = MNEMONIC.into();
        query.enc = Enc::Vop1;
        query.src = l.ray.iter().map(|&r| Opnd::Vgpr(r)).chain([
            l.min.map_or(Opnd::ConstInt(0), Opnd::Vgpr),
            Opnd::Sgpr(l.table),
            Opnd::Literal(l.scene),
        ]).collect();
        query.dst = if l.closest {
            vec![Opnd::Vgpr(l.hit), Opnd::Vgpr(l.ray[6])]
        } else {
            vec![Opnd::Vgpr(l.hit)]
        };
    }
}

/// Index of a title acceleration structure in the runtime's private scene
/// space: the title's structures ordered by (space, register).
fn scene_index(ex: &ShaderExtract, space: u32, register: u32) -> Option<usize> {
    let mut titles: Vec<(u32, u32)> =
        ex.resources.iter().filter(|r| r.res_type == 4 && r.lower == r.upper).map(|r| (r.space, r.lower)).collect();
    titles.sort_unstable();
    titles.iter().position(|&t| t == (space, register))
}

pub(super) fn emit(em: &mut Emitter<'_>, ins: &Insn) -> Result<(), String> {
    let v = |n: usize| match ins.src[n] {
        Opnd::Vgpr(r) => Ok(format!("v{r}")),
        _ => Err(format!("{MNEMONIC} at {:#x}: bad ray operand", ins.offset)),
    };
    let (Some(Opnd::Vgpr(hit)), Opnd::Sgpr(table), Opnd::Literal(offset)) = (ins.dst.first(), &ins.src[8], &ins.src[9])
    else {
        return Err(format!("{MNEMONIC} at {:#x}: bad operands", ins.offset));
    };
    let (kind, register, space) = em.table_binding_space(*table, *offset, 32)?;
    let scene = (kind == Some(0)).then(|| scene_index(em.ex, space, register)).flatten().ok_or_else(|| {
        format!("{MNEMONIC} at {:#x}: s{table} + {offset:#x} is not a title acceleration structure ({kind:?} t{register} space{space})", ins.offset)
    })?;
    let min = match ins.src[7] {
        Opnd::Vgpr(r) => format!("asfloat(v{r})"),
        _ => "0.0".into(),
    };
    if let Some(Opnd::Vgpr(distance)) = ins.dst.get(1) {
        // The walk keeps the nearest triangle with minimum <= t < maximum;
        // like the first-hit walks it tests no mask and culls nothing.
        em.line(&format!(
            "if (exec_lo & 1u) {{ float t; if (tbScarlettNearest(tbScarlettScene{scene}, asfloat(uint3({}, {}, {})), asfloat(uint3({}, {}, {})), {min}, asfloat({}), t)) {{ v{hit} = 1u; v{distance} = asuint(t); }} }}",
            v(0)?, v(1)?, v(2)?, v(3)?, v(4)?, v(5)?, v(6)?
        ));
        return Ok(());
    }
    // The walk accepts the first triangle with minimum <= t < maximum and
    // tests no instance mask. It also reads a facing-cull field from each
    // console instance's flags; the native query culls nothing, which matches
    // the walk for instances that leave that field clear.
    em.line(&format!(
        "if (exec_lo & 1u) {{ v{hit} = tbScarlettOccluded(tbScarlettScene{scene}, asfloat(uint3({}, {}, {})), asfloat(uint3({}, {}, {})), {min}, asfloat({})) ? 1u : 0u; }}",
        v(0)?, v(1)?, v(2)?, v(3)?, v(4)?, v(5)?, v(6)?
    ));
    Ok(())
}

/// Declares the scenes the emitted code reads, at their register in the
/// runtime's private space 900, and the query they share.
pub(super) fn declarations(hlsl: &str) -> String {
    let mut out = String::new();
    for i in 0..8 {
        if hlsl.contains(&format!("tbScarlettScene{i},")) {
            out += &format!("RaytracingAccelerationStructure tbScarlettScene{i} : register(t{i}, space900);\n");
        }
    }
    if hlsl.contains("tbScarlettOccluded(tbScarlett") {
        out += OCCLUDED;
    }
    if hlsl.contains("tbScarlettNearest(tbScarlett") {
        out += NEAREST;
    }
    out
}

const OCCLUDED: &str = r#"bool tbScarlettOccluded(RaytracingAccelerationStructure scene, float3 origin, float3 direction, float nearT, float farT) {
    if (!(farT > nearT) || !(nearT >= 0.0)) return false;
    RayDesc ray = {origin, nearT, direction, farT};
    RayQuery<RAY_FLAG_FORCE_OPAQUE | RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_PROCEDURAL_PRIMITIVES> query;
    query.TraceRayInline(scene, RAY_FLAG_NONE, 0xffu, ray);
    query.Proceed();
    return query.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
}
"#;

const NEAREST: &str = r#"bool tbScarlettNearest(RaytracingAccelerationStructure scene, float3 origin, float3 direction, float nearT, float farT, out float t) {
    t = farT;
    if (!(farT > nearT) || !(nearT >= 0.0)) return false;
    RayDesc ray = {origin, nearT, direction, farT};
    RayQuery<RAY_FLAG_FORCE_OPAQUE | RAY_FLAG_SKIP_PROCEDURAL_PRIMITIVES> query;
    query.TraceRayInline(scene, RAY_FLAG_NONE, 0xffu, ray);
    query.Proceed();
    if (query.CommittedStatus() != COMMITTED_TRIANGLE_HIT) return false;
    t = query.CommittedRayT();
    return true;
}
"#;

#[cfg(test)]
#[path = "../tests/hlsl/scarlett_rays.rs"]
mod tests;
