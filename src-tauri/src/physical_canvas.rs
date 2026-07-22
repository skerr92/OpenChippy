use crate::{
    physical_layout::{DeviceKind, PhysicalDevice, PhysicalLayer, PhysicalShape},
    technology::PhysicalRuleDeck,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const EPSILON: f64 = 1e-9;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObstructionType {
    Device,
    Metal,
    Poly,
    Diffusion,
    Contact,
    Via,
    PowerRail,
    KeepOut,
    BlockBoundary,
    ReservedRoutingChannel,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OccupancyOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CanvasBounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl CanvasBounds {
    fn from_shape(shape: &PhysicalShape, halo: f64) -> Self {
        Self {
            left: shape.x - shape.width / 2.0 - halo,
            top: shape.y - shape.height / 2.0 - halo,
            right: shape.x + shape.width / 2.0 + halo,
            bottom: shape.y + shape.height / 2.0 + halo,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.left < other.right - EPSILON
            && self.right > other.left + EPSILON
            && self.top < other.bottom - EPSILON
            && self.bottom > other.top + EPSILON
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OccupiedInterval {
    pub id: u64,
    pub layer: PhysicalLayer,
    pub start: f64,
    pub end: f64,
    pub cross_start: f64,
    pub cross_end: f64,
    pub orientation: OccupancyOrientation,
    pub tracks: Vec<i64>,
    pub owner: String,
    pub net: Option<usize>,
    pub obstruction_type: ObstructionType,
    pub spacing_halo_um: f64,
    #[serde(skip)]
    bounds: CanvasBounds,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanvasCollision {
    pub occupied_id: u64,
    pub owner: String,
    pub net: Option<usize>,
    pub obstruction_type: ObstructionType,
}

#[derive(Clone, Debug)]
pub struct PhysicalCanvas {
    rules: PhysicalRuleDeck,
    cell_size_um: f64,
    next_id: u64,
    occupancy: HashMap<PhysicalLayer, Vec<OccupiedInterval>>,
    track_index: HashMap<(PhysicalLayer, OccupancyOrientation, i64), Vec<u64>>,
    spatial_index: HashMap<(PhysicalLayer, i64, i64), Vec<u64>>,
}

impl PhysicalCanvas {
    pub fn new(rules: &PhysicalRuleDeck) -> Self {
        let largest_spacing = rules
            .diffusion
            .min_spacing_um
            .max(rules.poly.min_spacing_um)
            .max(rules.well.min_spacing_um)
            .max(rules.metal.min_spacing_um)
            .max(rules.contact.min_spacing_um)
            .max(rules.via.min_spacing_um);
        Self {
            rules: rules.clone(),
            cell_size_um: (largest_spacing * 4.0).max(0.5),
            next_id: 1,
            occupancy: HashMap::new(),
            track_index: HashMap::new(),
            spatial_index: HashMap::new(),
        }
    }

    pub fn can_place(&self, shape: &PhysicalShape) -> Result<(), Vec<CanvasCollision>> {
        self.check(shape, false)
    }

    pub fn can_route(&self, shape: &PhysicalShape) -> Result<(), Vec<CanvasCollision>> {
        self.check(shape, true)
    }

    pub fn commit(
        &mut self,
        shape: &PhysicalShape,
        owner: impl Into<String>,
        obstruction_type: ObstructionType,
    ) -> Result<u64, Vec<CanvasCollision>> {
        let routing = matches!(
            obstruction_type,
            ObstructionType::Metal | ObstructionType::PowerRail | ObstructionType::Via
        );
        if routing {
            self.can_route(shape)?;
        } else {
            self.can_place(shape)?;
        }
        Ok(self.index_unchecked(shape, owner, obstruction_type))
    }

    /// Adds existing physical IR to the spatial index without asserting that it
    /// is legal. DRC needs every shape indexed, including the shapes it is
    /// expected to diagnose.
    pub fn index_unchecked(
        &mut self,
        shape: &PhysicalShape,
        owner: impl Into<String>,
        obstruction_type: ObstructionType,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let spacing = self.spacing_for(shape.layer);
        let bounds = CanvasBounds::from_shape(shape, spacing / 2.0);
        let horizontal = shape.width >= shape.height;
        let orientation = if horizontal {
            OccupancyOrientation::Horizontal
        } else {
            OccupancyOrientation::Vertical
        };
        let cross_start = if horizontal {
            shape.y - shape.height / 2.0
        } else {
            shape.x - shape.width / 2.0
        };
        let cross_end = if horizontal {
            shape.y + shape.height / 2.0
        } else {
            shape.x + shape.width / 2.0
        };
        let tracks = self.track_indices(cross_start, cross_end);
        let interval = OccupiedInterval {
            id,
            layer: shape.layer,
            start: if horizontal {
                shape.x - shape.width / 2.0
            } else {
                shape.y - shape.height / 2.0
            },
            end: if horizontal {
                shape.x + shape.width / 2.0
            } else {
                shape.y + shape.height / 2.0
            },
            cross_start,
            cross_end,
            orientation,
            tracks: tracks.clone(),
            owner: owner.into(),
            net: shape.net,
            obstruction_type,
            spacing_halo_um: spacing / 2.0,
            bounds,
        };
        self.index(id, shape.layer, bounds);
        for track in tracks {
            self.track_index
                .entry((shape.layer, orientation, track))
                .or_default()
                .push(id);
        }
        self.occupancy
            .entry(shape.layer)
            .or_default()
            .push(interval);
        id
    }

    pub fn commit_batch(
        &mut self,
        entries: &[(PhysicalShape, String, ObstructionType)],
    ) -> Result<Vec<u64>, Vec<CanvasCollision>> {
        let mut candidate = self.clone();
        let mut ids = Vec::with_capacity(entries.len());
        for (shape, owner, obstruction_type) in entries {
            ids.push(candidate.commit(shape, owner.clone(), *obstruction_type)?);
        }
        *self = candidate;
        Ok(ids)
    }

    pub fn commit_routing_geometry(
        &mut self,
        shapes: &[PhysicalShape],
        owner: impl Into<String>,
    ) -> Result<Vec<u64>, Vec<CanvasCollision>> {
        let owner = owner.into();
        let entries = shapes
            .iter()
            .cloned()
            .map(|shape| {
                let obstruction = match shape.layer {
                    PhysicalLayer::Metal(_) => ObstructionType::Metal,
                    PhysicalLayer::Via(_) => ObstructionType::Via,
                    _ => ObstructionType::KeepOut,
                };
                (shape, owner.clone(), obstruction)
            })
            .collect::<Vec<_>>();
        if entries.iter().any(|(_, _, obstruction)| {
            !matches!(obstruction, ObstructionType::Metal | ObstructionType::Via)
        }) {
            return Err(Vec::new());
        }
        if let [(shape, owner, obstruction)] = entries.as_slice() {
            return self
                .commit(shape, owner.clone(), *obstruction)
                .map(|id| vec![id]);
        }
        let mut preflight_collisions = Vec::new();
        for (shape, _, _) in &entries {
            if let Err(collisions) = self.can_route(shape) {
                preflight_collisions.extend(collisions);
            }
        }
        preflight_collisions.sort_by_key(|collision| collision.occupied_id);
        preflight_collisions.dedup_by_key(|collision| collision.occupied_id);
        if !preflight_collisions.is_empty() {
            return Err(preflight_collisions);
        }
        let mut committed = Vec::with_capacity(entries.len());
        for (shape, owner, obstruction) in &entries {
            match self.commit(shape, owner.clone(), *obstruction) {
                Ok(id) => committed.push(id),
                Err(collisions) => {
                    for id in committed.iter().rev() {
                        self.remove(*id);
                    }
                    return Err(collisions);
                }
            }
        }
        Ok(committed)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let mut removed = false;
        for intervals in self.occupancy.values_mut() {
            let before = intervals.len();
            intervals.retain(|interval| interval.id != id);
            removed |= intervals.len() != before;
        }
        if removed {
            self.rebuild_index();
        }
        removed
    }

    pub fn query_neighbors(&self, shape: &PhysicalShape) -> Vec<&OccupiedInterval> {
        let bounds = CanvasBounds::from_shape(shape, self.spacing_for(shape.layer) / 2.0);
        self.neighbor_ids(shape.layer, bounds)
            .into_iter()
            .filter_map(|id| {
                self.occupancy
                    .get(&shape.layer)?
                    .iter()
                    .find(|interval| interval.id == id)
            })
            .collect()
    }

    pub fn occupied_count(&self) -> usize {
        self.occupancy.values().map(Vec::len).sum()
    }

    pub fn occupied_on_layer(&self, layer: PhysicalLayer) -> &[OccupiedInterval] {
        self.occupancy.get(&layer).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn query_track(
        &self,
        layer: PhysicalLayer,
        orientation: OccupancyOrientation,
        coordinate_um: f64,
    ) -> Vec<&OccupiedInterval> {
        let track = (coordinate_um / self.rules.manufacturing_grid_um).floor() as i64;
        self.track_index
            .get(&(layer, orientation, track))
            .into_iter()
            .flatten()
            .filter_map(|id| {
                self.occupancy
                    .get(&layer)?
                    .iter()
                    .find(|interval| interval.id == *id)
            })
            .collect()
    }

    pub fn query_point(
        &self,
        layer: PhysicalLayer,
        x: f64,
        y: f64,
        tolerance_um: f64,
    ) -> Vec<&OccupiedInterval> {
        let query = CanvasBounds {
            left: x - tolerance_um,
            top: y - tolerance_um,
            right: x + tolerance_um,
            bottom: y + tolerance_um,
        };
        self.neighbor_ids(layer, query)
            .into_iter()
            .filter_map(|id| {
                self.occupancy
                    .get(&layer)?
                    .iter()
                    .find(|interval| interval.id == id)
            })
            .filter(|interval| {
                let physical = if interval.orientation == OccupancyOrientation::Horizontal {
                    CanvasBounds {
                        left: interval.start,
                        top: interval.cross_start,
                        right: interval.end,
                        bottom: interval.cross_end,
                    }
                } else {
                    CanvasBounds {
                        left: interval.cross_start,
                        top: interval.start,
                        right: interval.cross_end,
                        bottom: interval.end,
                    }
                };
                physical.overlaps(query)
            })
            .collect()
    }

    fn check(
        &self,
        shape: &PhysicalShape,
        allow_same_net_merge: bool,
    ) -> Result<(), Vec<CanvasCollision>> {
        let candidate = CanvasBounds::from_shape(shape, self.spacing_for(shape.layer) / 2.0);
        let mut collisions = self
            .query_neighbors(shape)
            .into_iter()
            .filter(|occupied| {
                if allow_same_net_merge
                    && shape.net.is_some()
                    && shape.net == occupied.net
                    && matches!(shape.layer, PhysicalLayer::Metal(_))
                    && matches!(
                        occupied.obstruction_type,
                        ObstructionType::Metal | ObstructionType::PowerRail
                    )
                {
                    return false;
                }
                candidate.overlaps(occupied.bounds)
            })
            .map(|occupied| CanvasCollision {
                occupied_id: occupied.id,
                owner: occupied.owner.clone(),
                net: occupied.net,
                obstruction_type: occupied.obstruction_type,
            })
            .collect::<Vec<_>>();
        collisions.sort_by_key(|collision| collision.occupied_id);
        collisions.dedup_by_key(|collision| collision.occupied_id);
        if collisions.is_empty() {
            Ok(())
        } else {
            Err(collisions)
        }
    }

    fn spacing_for(&self, layer: PhysicalLayer) -> f64 {
        match layer {
            PhysicalLayer::Nwell => self.rules.well.min_spacing_um,
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => self.rules.diffusion.min_spacing_um,
            PhysicalLayer::Poly => self.rules.poly.min_spacing_um,
            PhysicalLayer::Metal(index) => {
                self.rules
                    .layer_overrides
                    .get(&format!("metal{index}"))
                    .unwrap_or(&self.rules.metal)
                    .min_spacing_um
            }
            PhysicalLayer::Contact => self.rules.contact.min_spacing_um,
            PhysicalLayer::Via(lower) => {
                self.rules
                    .via_overrides
                    .get(&format!("via{lower}{}", lower + 1))
                    .unwrap_or(&self.rules.via)
                    .min_spacing_um
            }
            PhysicalLayer::Substrate => 0.0,
        }
    }

    fn cells(&self, bounds: CanvasBounds) -> impl Iterator<Item = (i64, i64)> {
        let min_x = (bounds.left / self.cell_size_um).floor() as i64;
        let max_x = (bounds.right / self.cell_size_um).floor() as i64;
        let min_y = (bounds.top / self.cell_size_um).floor() as i64;
        let max_y = (bounds.bottom / self.cell_size_um).floor() as i64;
        (min_x..=max_x).flat_map(move |x| (min_y..=max_y).map(move |y| (x, y)))
    }

    fn track_indices(&self, start: f64, end: f64) -> Vec<i64> {
        let grid = self.rules.manufacturing_grid_um;
        let first = (start.min(end) / grid).floor() as i64;
        let last = (start.max(end) / grid).floor() as i64;
        (first..=last).collect()
    }

    fn index(&mut self, id: u64, layer: PhysicalLayer, bounds: CanvasBounds) {
        let cells = self.cells(bounds).collect::<Vec<_>>();
        for (x, y) in cells {
            self.spatial_index
                .entry((layer, x, y))
                .or_default()
                .push(id);
        }
    }

    fn neighbor_ids(&self, layer: PhysicalLayer, bounds: CanvasBounds) -> Vec<u64> {
        let mut ids = HashSet::new();
        for (x, y) in self.cells(bounds) {
            if let Some(indexed) = self.spatial_index.get(&(layer, x, y)) {
                ids.extend(indexed);
            }
        }
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    fn rebuild_index(&mut self) {
        let entries = self
            .occupancy
            .values()
            .flat_map(|intervals| {
                intervals.iter().map(|interval| {
                    (
                        interval.id,
                        interval.layer,
                        interval.bounds,
                        interval.orientation,
                        interval.tracks.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        self.spatial_index.clear();
        self.track_index.clear();
        for (id, layer, bounds, orientation, tracks) in entries {
            self.index(id, layer, bounds);
            for track in tracks {
                self.track_index
                    .entry((layer, orientation, track))
                    .or_default()
                    .push(id);
            }
        }
    }
}

pub fn device_footprint(
    device: &PhysicalDevice,
    x: f64,
    y: f64,
    rules: &PhysicalRuleDeck,
) -> Vec<(PhysicalShape, ObstructionType)> {
    let diffusion_height = 0.55 + device.width_um.min(4.0) * 0.12;
    let diffusion = PhysicalShape {
        layer: if device.kind == DeviceKind::Pmos {
            PhysicalLayer::Pdiff
        } else {
            PhysicalLayer::Ndiff
        },
        x,
        y,
        width: 1.55,
        height: diffusion_height,
        component_id: Some(device.component_id),
        net: None,
    };
    let poly = PhysicalShape {
        layer: PhysicalLayer::Poly,
        x,
        y,
        width: 0.22_f64.max(device.length_um * 0.22),
        height: 1.25_f64.max(diffusion_height + rules.gate_extension_um * 2.0),
        component_id: Some(device.component_id),
        net: Some(device.gate_net),
    };
    let drain = PhysicalShape {
        layer: PhysicalLayer::Contact,
        x: x - 0.58,
        y,
        width: rules.contact.size_um,
        height: rules.contact.size_um,
        component_id: Some(device.component_id),
        net: Some(device.drain_net),
    };
    let source = PhysicalShape {
        layer: PhysicalLayer::Contact,
        x: x + 0.58,
        net: Some(device.source_net),
        ..drain.clone()
    };
    let mut footprint = vec![
        (diffusion, ObstructionType::Diffusion),
        (poly, ObstructionType::Poly),
        (drain, ObstructionType::Contact),
        (source, ObstructionType::Contact),
    ];
    for (shape, _) in &mut footprint {
        snap_shape_to_grid(shape, rules.manufacturing_grid_um);
    }
    footprint
}

fn snap_shape_to_grid(shape: &mut PhysicalShape, grid: f64) {
    let snap = |value: f64| (value / grid).round() * grid;
    let left = snap(shape.x - shape.width / 2.0);
    let top = snap(shape.y - shape.height / 2.0);
    let right = snap(shape.x + shape.width / 2.0);
    let bottom = snap(shape.y + shape.height / 2.0);
    shape.x = (left + right) / 2.0;
    shape.y = (top + bottom) / 2.0;
    shape.width = (right - left).max(grid);
    shape.height = (bottom - top).max(grid);
}

pub fn reserve_device_footprint(
    canvas: &mut PhysicalCanvas,
    device: &PhysicalDevice,
    x: f64,
    y: f64,
    rules: &PhysicalRuleDeck,
) -> Result<Vec<u64>, Vec<CanvasCollision>> {
    let entries = device_footprint(device, x, y, rules)
        .into_iter()
        .map(|(shape, obstruction)| (shape, device.name.clone(), obstruction))
        .collect::<Vec<_>>();
    canvas.commit_batch(&entries)
}

#[cfg(test)]
mod tests {
    use super::{reserve_device_footprint, ObstructionType, OccupancyOrientation, PhysicalCanvas};
    use crate::{
        physical_layout::{DeviceKind, PhysicalDevice, PhysicalLayer, PhysicalShape},
        technology::PhysicalRuleDeck,
    };
    use uuid::Uuid;

    fn shape(layer: PhysicalLayer, x: f64, net: Option<usize>) -> PhysicalShape {
        PhysicalShape {
            layer,
            x,
            y: 0.0,
            width: 0.2,
            height: 1.0,
            component_id: None,
            net,
        }
    }

    #[test]
    fn occupancy_is_layer_aware_and_enforces_spacing_halos() {
        let mut canvas = PhysicalCanvas::new(&PhysicalRuleDeck::default());
        let first = shape(PhysicalLayer::Metal(1), 0.0, Some(1));
        let too_close = shape(PhysicalLayer::Metal(1), 0.23, Some(2));
        let other_layer = shape(PhysicalLayer::Metal(2), 0.0, Some(2));
        canvas
            .commit(&first, "net-1", ObstructionType::Metal)
            .unwrap();
        assert!(canvas.can_route(&too_close).is_err());
        assert!(canvas.can_route(&other_layer).is_ok());
    }

    #[test]
    fn same_net_metal_can_merge_but_vias_cannot() {
        let mut canvas = PhysicalCanvas::new(&PhysicalRuleDeck::default());
        let first = shape(PhysicalLayer::Metal(1), 0.0, Some(1));
        let overlap = shape(PhysicalLayer::Metal(1), 0.0, Some(1));
        canvas
            .commit(&first, "trunk", ObstructionType::Metal)
            .unwrap();
        assert!(canvas.can_route(&overlap).is_ok());

        let via = shape(PhysicalLayer::Via(1), 0.0, Some(1));
        canvas.commit(&via, "via-a", ObstructionType::Via).unwrap();
        assert!(canvas.can_route(&via).is_err());
    }

    #[test]
    fn committed_geometry_can_be_queried_and_removed() {
        let mut canvas = PhysicalCanvas::new(&PhysicalRuleDeck::default());
        let device = shape(PhysicalLayer::Ndiff, 0.0, None);
        let id = canvas
            .commit(&device, "M1", ObstructionType::Device)
            .unwrap();
        assert_eq!(canvas.occupied_count(), 1);
        assert_eq!(canvas.query_neighbors(&device)[0].owner, "M1");
        assert!(canvas.remove(id));
        assert!(canvas.query_neighbors(&device).is_empty());
    }

    #[test]
    fn occupancy_can_be_inspected_by_layer_and_process_track() {
        let mut canvas = PhysicalCanvas::new(&PhysicalRuleDeck::default());
        let horizontal = PhysicalShape {
            layer: PhysicalLayer::Metal(3),
            x: 1.0,
            y: 0.25,
            width: 2.0,
            height: 0.2,
            component_id: None,
            net: Some(7),
        };
        canvas
            .commit(&horizontal, "clock-trunk", ObstructionType::Metal)
            .unwrap();

        let layer = canvas.occupied_on_layer(PhysicalLayer::Metal(3));
        assert_eq!(layer.len(), 1);
        assert_eq!(layer[0].orientation, OccupancyOrientation::Horizontal);
        assert!(layer[0].tracks.len() > 1);
        let track = canvas.query_track(
            PhysicalLayer::Metal(3),
            OccupancyOrientation::Horizontal,
            0.25,
        );
        assert_eq!(track.len(), 1);
        assert_eq!(track[0].owner, "clock-trunk");
        assert!(canvas
            .query_track(
                PhysicalLayer::Metal(2),
                OccupancyOrientation::Horizontal,
                0.25,
            )
            .is_empty());
    }

    #[test]
    fn device_footprints_commit_atomically() {
        let rules = PhysicalRuleDeck::default();
        let mut canvas = PhysicalCanvas::new(&rules);
        let device = PhysicalDevice {
            component_id: Uuid::new_v4(),
            name: "M1".into(),
            physical_group: None,
            kind: DeviceKind::Nmos,
            gate_net: 1,
            drain_net: 2,
            source_net: 3,
            width_um: 1.0,
            length_um: 1.0,
        };
        reserve_device_footprint(&mut canvas, &device, 0.0, 0.0, &rules).unwrap();
        assert_eq!(canvas.occupied_count(), 4);

        let mut colliding = device.clone();
        colliding.component_id = Uuid::new_v4();
        colliding.name = "M2".into();
        assert!(reserve_device_footprint(&mut canvas, &colliding, 0.0, 0.0, &rules).is_err());
        assert_eq!(
            canvas.occupied_count(),
            4,
            "failed footprint must not partially commit"
        );
    }

    #[test]
    fn routing_geometry_commits_atomically() {
        let rules = PhysicalRuleDeck::default();
        let mut canvas = PhysicalCanvas::new(&rules);
        let first = shape(PhysicalLayer::Metal(2), 0.0, Some(1));
        canvas.commit_routing_geometry(&[first], "net-1").unwrap();
        let legal = shape(PhysicalLayer::Metal(3), 0.0, Some(2));
        let illegal = shape(PhysicalLayer::Metal(2), 0.0, Some(2));
        assert!(canvas
            .commit_routing_geometry(&[legal, illegal], "net-2")
            .is_err());
        assert_eq!(canvas.occupied_count(), 1);
        assert!(canvas.occupied_on_layer(PhysicalLayer::Metal(3)).is_empty());
    }

    #[test]
    fn unchecked_geometry_remains_spatially_queryable_for_drc_and_hit_testing() {
        let rules = PhysicalRuleDeck::default();
        let mut canvas = PhysicalCanvas::new(&rules);
        let first = shape(PhysicalLayer::Metal(2), 0.0, Some(1));
        let second = shape(PhysicalLayer::Metal(2), 0.05, Some(2));
        canvas.index_unchecked(&first, "illegal-a", ObstructionType::Metal);
        canvas.index_unchecked(&second, "illegal-b", ObstructionType::Metal);
        assert_eq!(canvas.query_neighbors(&first).len(), 2);
        let hits = canvas.query_point(PhysicalLayer::Metal(2), 0.0, 0.0, 0.01);
        assert_eq!(hits.len(), 2);
    }
}
