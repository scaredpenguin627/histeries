use crate::{Command, Feature, MapState, PlayerId, ResourceKind, RiverDirection, Terrain};
use histeries_hex::{TileId, TileTopology};
use wasmi::{Caller, Error, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};

const ABI: &str = "histeries_editor_v1";
const MAX_EDITS: usize = 128;
const FUEL: u64 = 100_000;

pub(super) struct Guest<'a> {
    input: Option<(PlayerId, &'a MapState)>,
    pub(super) commands: Vec<Command>,
    limits: StoreLimits,
}

impl<'a> Guest<'a> {
    fn input(&self) -> Result<(PlayerId, &'a MapState), Error> {
        self.input.ok_or_else(|| Error::new("map is unavailable"))
    }
}

type Ready<'a> = (Store<Guest<'a>>, TypedFunc<(), i32>);

pub(super) fn prepare<'a>(
    module: &Module,
    input: Option<(PlayerId, &'a MapState)>,
) -> Result<Ready<'a>, Error> {
    let engine = module.engine();
    let limits = StoreLimitsBuilder::new()
        .instances(1)
        .memories(1)
        .memory_size(256 * 1024)
        .tables(1)
        .table_elements(64)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(
        engine,
        Guest {
            input,
            limits,
            commands: Vec::new(),
        },
    );
    store.limiter(|guest| &mut guest.limits);
    store.set_fuel(FUEL)?;
    let mut linker = Linker::new(engine);
    linker.func_wrap(ABI, "tile_count", |caller: Caller<'_, Guest<'_>>| {
        i32::try_from(caller.data().input()?.1.topology().tile_count())
            .map_err(|_| Error::new("map too large for ABI v1"))
    })?;
    linker.func_wrap(ABI, "neighbor", neighbor)?;
    linker.func_wrap(ABI, "read", read)?;
    linker.func_wrap(ABI, "emit", emit)?;
    let instance = linker.instantiate_and_start(&mut store, module)?;
    let entry = instance.get_typed_func::<(), i32>(&store, "apply")?;
    Ok((store, entry))
}

fn tile(raw: i32) -> Result<TileId, Error> {
    Ok(TileId::new(
        u32::try_from(raw).map_err(|_| Error::new("negative tile ID"))?,
    ))
}

fn neighbor(caller: Caller<'_, Guest<'_>>, raw: i32, slot: i32) -> Result<i32, Error> {
    let map = caller.data().input()?.1;
    let tile = tile(raw)?;
    if map.tile(tile).is_none() || !(0..6).contains(&slot) {
        return Err(Error::new("invalid tile or neighbor slot"));
    }
    Ok(map
        .topology()
        .neighbors(tile)
        .get(slot as usize)
        .map_or(-1, |id| id.index() as i32))
}

// Numeric positions are part of ABI v1 and independent of map serialization.
const TERRAINS: [Terrain; 8] = [
    Terrain::Plains,
    Terrain::Grassland,
    Terrain::Desert,
    Terrain::Tundra,
    Terrain::Coast,
    Terrain::Ocean,
    Terrain::Hill,
    Terrain::Mountain,
];
const FEATURES: [Feature; 4] = [
    Feature::None,
    Feature::Forest,
    Feature::Jungle,
    Feature::Marsh,
];
const RIVERS: [RiverDirection; 6] = [
    RiverDirection::NorthEast,
    RiverDirection::East,
    RiverDirection::SouthEast,
    RiverDirection::SouthWest,
    RiverDirection::West,
    RiverDirection::NorthWest,
];
const RESOURCES: [ResourceKind; 4] = [
    ResourceKind::Wheat,
    ResourceKind::Stone,
    ResourceKind::Iron,
    ResourceKind::Gold,
];

fn encode<T: PartialEq>(codes: &[T], value: T) -> Result<i32, Error> {
    codes
        .iter()
        .position(|candidate| *candidate == value)
        .map(|i| i as i32)
        .ok_or_else(|| Error::new("tile value is not representable in ABI v1"))
}

fn decode<T: Copy>(codes: &[T], raw: i32) -> Result<T, Error> {
    usize::try_from(raw)
        .ok()
        .and_then(|index| codes.get(index))
        .copied()
        .ok_or_else(|| Error::new("invalid tile field or value"))
}

fn read(caller: Caller<'_, Guest<'_>>, raw: i32, field: i32) -> Result<i32, Error> {
    let map = caller.data().input()?.1;
    let t = map
        .tile(tile(raw)?)
        .ok_or_else(|| Error::new("unknown tile"))?;
    match field {
        0 => encode(&TERRAINS, t.terrain),
        1 => encode(&FEATURES, t.feature),
        2 => t.river.map_or(Ok(-1), |v| encode(&RIVERS, v)),
        3 => t.resource.map_or(Ok(-1), |v| encode(&RESOURCES, v)),
        _ => Err(Error::new("unknown tile field")),
    }
}

fn emit(mut caller: Caller<'_, Guest<'_>>, raw: i32, field: i32, value: i32) -> Result<(), Error> {
    let guest = caller.data_mut();
    if guest.commands.len() >= MAX_EDITS {
        return Err(Error::new("package edit limit exceeded"));
    }
    let (player, _) = guest.input()?;
    let tile = tile(raw)?;
    let edit = match field {
        0 => Command::EditorSetTerrain {
            player,
            tile,
            terrain: decode(&TERRAINS, value)?,
        },
        1 => Command::EditorSetFeature {
            player,
            tile,
            feature: decode(&FEATURES, value)?,
        },
        2 => Command::EditorSetRiver {
            player,
            tile,
            river: if value == -1 {
                None
            } else {
                Some(decode(&RIVERS, value)?)
            },
        },
        3 => Command::EditorSetResource {
            player,
            tile,
            resource: if value == -1 {
                None
            } else {
                Some(decode(&RESOURCES, value)?)
            },
        },
        _ => return Err(Error::new("invalid tile field or value")),
    };
    guest.commands.push(edit);
    Ok(())
}
