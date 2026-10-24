//! A disposable area/terminal projection of immutable device rows, shared across actors.
use super::*;
use game_storage::Version;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Outlet {
    pub eligible: bool,
    pub memory: String,
    pub offer: String,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Device {
    pub id: u64,
    pub outlets: Vec<Outlet>,
}
struct Index {
    version: Version<u64, Arc<game_mechanics::Device>>,
    areas: BTreeMap<AreaId, Arc<Vec<Device>>>,
    empty: Arc<Vec<Device>>,
}
thread_local! { static INDEX: RefCell<Option<Index>> = const { RefCell::new(None) }; }
pub(super) fn local(world: &Machines, area: AreaId) -> Arc<Vec<Device>> {
    INDEX.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot
            .as_ref()
            .is_none_or(|i| !world.devices.matches(&i.version))
        {
            let mut areas: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for (id, d) in world.devices.iter() {
                let outlets = (0..d.parts.len())
                    .map(|outlet| Outlet {
                        eligible: eligible(d, outlet),
                        memory: format!("mechanics:device:{id}:{outlet}"),
                        offer: format!("mechanics:test:{id}:{outlet}"),
                    })
                    .collect();
                areas
                    .entry(d.area)
                    .or_default()
                    .push(Device { id: *id, outlets });
            }
            *slot = Some(Index {
                version: world.devices.version(),
                empty: Arc::default(),
                areas: areas.into_iter().map(|(a, d)| (a, Arc::new(d))).collect(),
            });
        }
        let index = slot.as_ref().unwrap();
        index.areas.get(&area).unwrap_or(&index.empty).clone()
    })
}
pub(super) fn eligible(d: &game_mechanics::Device, outlet: usize) -> bool {
    d.parts[outlet].mechanism.output() == game_physics::Motion::Linear
        && !d.parts.iter().any(|p| p.input == Some(outlet))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn machine(area: AreaId) -> game_mechanics::Device {
        game_mechanics::Device {
            area,
            inventory: InventoryId(1),
            maker: PersonId(1),
            broken: Default::default(),
            parts: vec![game_physics::Part {
                mechanism: game_physics::Mechanism::Lever,
                good: GoodId(1),
                input: None,
                drive: 2,
                load: 1,
            }],
        }
    }
    #[test]
    fn projection_reuses_rows_and_tracks_geometry_area_worlds_and_rollback() {
        let area = AreaId(1);
        let mut world = Machines::default();
        world.devices.insert(7, machine(area));
        let before = world.clone();
        let first = local(&world, area);
        assert!(Arc::ptr_eq(&first, &local(&world, area)));
        assert!(first[0].outlets[0].eligible);
        let d = world.devices.get_mut(&7).unwrap();
        d.area = AreaId(2);
        let mut next = d.parts[0].clone();
        next.input = Some(0);
        d.parts.push(next);
        assert!(local(&world, area).is_empty());
        let changed = local(&world, AreaId(2));
        assert!(!changed[0].outlets[0].eligible && changed[0].outlets[1].eligible);
        assert_eq!(local(&before, area), first);
        let mut other = Machines::default();
        other.devices.insert(8, machine(area));
        assert_eq!(local(&other, area)[0].id, 8);
        assert_eq!(local(&before, area), first);
    }
}
