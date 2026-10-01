//! Repeated, electrically inert fill. Arrays retain exact tile geometry while
//! density measurements and keepout checks operate without expanding tiles.
use crate::physical_layout::{
    density_fill_layer, density_fill_obstacle, rectangle_union_area, sliding_density_windows,
    DensitySpatialIndex, PhysicalBounds, PhysicalDensityLayerReport, PhysicalDensityReport,
    PhysicalLayer, PhysicalShape, PhysicalShapePurpose,
};
use crate::technology::Technology;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DensityArray {
    pub material: String,
    pub layer: PhysicalLayer,
    /// Center of the first tile; subsequent tiles advance in +X and +Y.
    pub x: f64,
    pub y: f64,
    pub tile_width: f64,
    pub tile_height: f64,
    pub step_x: f64,
    pub step_y: f64,
    pub columns: u32,
    pub rows: u32,
}

impl DensityArray {
    pub fn tile_count(&self) -> u64 {
        u64::from(self.columns) * u64::from(self.rows)
    }

    pub fn envelope(&self) -> PhysicalShape {
        let dx = f64::from(self.columns.saturating_sub(1)) * self.step_x;
        let dy = f64::from(self.rows.saturating_sub(1)) * self.step_y;
        PhysicalShape {
            layer: self.layer,
            x: self.x + dx / 2.0,
            y: self.y + dy / 2.0,
            width: self.tile_width + dx,
            height: self.tile_height + dy,
            component_id: None,
            net: None,
            purpose: PhysicalShapePurpose::DummyFill,
        }
    }

    pub fn area_in(&self, bounds: &PhysicalBounds) -> f64 {
        // The area of a Cartesian lattice factors into two sums of clipped
        // intervals. Only the first and last overlapping tiles are partial.
        fn length(origin: f64, size: f64, step: f64, count: u32, lo: f64, hi: f64) -> f64 {
            if count == 0 || step <= 0.0 || hi <= lo { return 0.0; }
            let left = origin - size / 2.0;
            let first = (((lo - left - size) / step).floor() + 1.0).max(0.0) as u32;
            let end = ((hi - left) / step).ceil().max(0.0).min(f64::from(count)) as u32;
            if first >= end { return 0.0; }
            let clip = |index: u32| {
                let start = left + f64::from(index) * step;
                (hi.min(start + size) - lo.max(start)).max(0.0)
            };
            if end - first == 1 { clip(first) }
            else { clip(first) + clip(end - 1) + f64::from(end - first - 2) * size }
        }
        length(self.x, self.tile_width, self.step_x, self.columns, bounds.min_x, bounds.max_x)
            * length(self.y, self.tile_height, self.step_y, self.rows, bounds.min_y, bounds.max_y)
    }
}

pub fn generate(shapes: &[PhysicalShape], bounds: &PhysicalBounds, technology: &Technology)
    -> Result<(Vec<DensityArray>, PhysicalDensityReport), String>
{
    let rules = &technology.physical_rules.density_fill;
    let grid = technology.physical_rules.manufacturing_grid_um;
    let mut spatial = DensitySpatialIndex::new(64.0);
    for (index, shape) in shapes.iter().enumerate() { spatial.insert(index, shape); }
    let mut arrays: Vec<DensityArray> = Vec::new();
    let mut pending = rules.layers.keys().cloned().collect::<Vec<_>>();
    // Resolve support dependencies explicitly; their lattice is shared exactly.
    let mut completed = std::collections::BTreeSet::new();
    while !pending.is_empty() {
        let Some(index) = pending.iter().position(|name| rules.layers[name].support_layer.as_ref()
            .is_none_or(|support| completed.contains(support))) else {
                return Err("Density array support layers contain a cycle or missing material".into());
            };
        let material = pending.remove(index);
        let rule = &rules.layers[&material];
        let layer = density_fill_layer(&material, technology).ok_or("Invalid fill layer")?;
        let mut seeds = if let Some(support) = &rule.support_layer {
            arrays.iter().filter(|array| &array.material == support).map(|array| DensityArray {
                material: material.clone(), layer,
                tile_width: rule.tile_width_um, tile_height: rule.tile_height_um,
                ..array.clone()
            }).collect::<Vec<_>>()
        } else {
            let step_x = ((rule.tile_width_um + rule.fill_spacing_um) / grid).ceil() * grid;
            let step_y = ((rule.tile_height_um + rule.fill_spacing_um) / grid).ceil() * grid;
            let columns = (((bounds.max_x - bounds.min_x - rule.tile_width_um) / step_x).floor() + 1.0).max(0.0) as u32;
            let rows = (((bounds.max_y - bounds.min_y - rule.tile_height_um) / step_y).floor() + 1.0).max(0.0) as u32;
            vec![DensityArray { material: material.clone(), layer,
                x: bounds.min_x + rule.tile_width_um / 2.0,
                y: bounds.min_y + rule.tile_height_um / 2.0,
                tile_width: rule.tile_width_um, tile_height: rule.tile_height_um,
                step_x, step_y, columns, rows }]
        };
        let halo = rule.circuit_spacing_um.max(rule.ring_keepout_um).max(rule.fill_spacing_um);
        while let Some(array) = seeds.pop() {
            if array.columns == 0 || array.rows == 0 { continue; }
            let envelope = array.envelope();
            let blocked = spatial.query(&envelope, halo).into_iter()
                .any(|index| density_fill_obstacle(&material, &envelope, &shapes[index], rule));
            if !blocked && array.columns <= 32767 && array.rows <= 32767 {
                arrays.push(array);
            } else if array.columns > 1 && (array.columns >= array.rows || array.rows == 1) {
                let half = array.columns / 2;
                seeds.push(DensityArray { columns: half, ..array.clone() });
                seeds.push(DensityArray { x: array.x + f64::from(half) * array.step_x,
                    columns: array.columns - half, ..array });
            } else if array.rows > 1 {
                let half = array.rows / 2;
                seeds.push(DensityArray { rows: half, ..array.clone() });
                seeds.push(DensityArray { y: array.y + f64::from(half) * array.step_y,
                    rows: array.rows - half, ..array });
            }
        }
        completed.insert(material);
    }
    arrays.sort_by(|a,b| a.material.cmp(&b.material).then_with(|| a.y.total_cmp(&b.y)).then_with(|| a.x.total_cmp(&b.x)));
    let report = measure(shapes, &arrays, bounds, technology);
    Ok((arrays, report))
}

pub fn measure(shapes: &[PhysicalShape], arrays: &[DensityArray], bounds: &PhysicalBounds,
    technology: &Technology) -> PhysicalDensityReport
{
    let rules = &technology.physical_rules.density_fill;
    let windows = sliding_density_windows(bounds, rules.evaluation_window_width_um,
        rules.evaluation_window_height_um, rules.evaluation_window_step_x_um,
        rules.evaluation_window_step_y_um);
    let mut report = PhysicalDensityReport::default();
    for (material, rule) in &rules.layers {
        let Some(layer) = density_fill_layer(material, technology) else { continue; };
        let relevant = arrays.iter().filter(|a| &a.material == material).collect::<Vec<_>>();
        let density = |window: &PhysicalBounds| {
            let area = rectangle_union_area(shapes, material, layer, window)
                + relevant.iter().map(|array| array.area_in(window)).sum::<f64>();
            area / ((window.max_x-window.min_x)*(window.max_y-window.min_y))
        };
        let global = density(bounds);
        let local = windows.iter().map(density).collect::<Vec<_>>();
        let minimum_global = rule.minimum_global_density.unwrap_or(rule.target_density);
        let minimum_window = rule.minimum_window_density.unwrap_or(minimum_global);
        let maximum_global = rule.maximum_global_density.or(rule.maximum_density);
        let maximum_window = rule.maximum_window_density.or(maximum_global);
        report.layers.push(PhysicalDensityLayerReport {
            material: material.clone(), preferred_density: rule.target_density,
            minimum_global_density: minimum_global, maximum_global_density: maximum_global,
            achieved_global_density: global, minimum_window_density: minimum_window,
            maximum_window_density: maximum_window,
            achieved_minimum_window_density: local.iter().copied().min_by(f64::total_cmp).unwrap_or(global),
            achieved_maximum_window_density: local.iter().copied().max_by(f64::total_cmp).unwrap_or(global),
            window_width_um: windows.first().map_or(0.0, |w| w.max_x-w.min_x),
            window_height_um: windows.first().map_or(0.0, |w| w.max_y-w.min_y),
            window_step_x_um: rules.evaluation_window_step_x_um.or(rules.evaluation_window_width_um.map(|w| w/2.0)).unwrap_or(0.0),
            window_step_y_um: rules.evaluation_window_step_y_um.or(rules.evaluation_window_height_um.map(|h| h/2.0)).unwrap_or(0.0),
            window_count: windows.len(),
            underfilled_window_count: local.iter().filter(|v| **v+1e-9 < minimum_window).count(),
            overfilled_window_count: maximum_window.map_or(0, |max| local.iter().filter(|v| **v > max+1e-9).count()),
            dummy_shape_count: relevant.iter().map(|a| a.tile_count() as usize).sum(),
        });
    }
    report
}

pub fn validate(arrays: &[DensityArray], shapes: &[PhysicalShape], bounds: &PhysicalBounds,
    technology: &Technology) -> Vec<(PhysicalLayer, String)>
{
    let mut issues = Vec::new();
    let grid = technology.physical_rules.manufacturing_grid_um;
    let mut spatial = DensitySpatialIndex::new(64.0);
    for (index, shape) in shapes.iter().enumerate() { spatial.insert(index, shape); }
    let envelopes = arrays.iter().map(DensityArray::envelope).collect::<Vec<_>>();
    let mut prior = DensitySpatialIndex::new(64.0);
    for (index, array) in arrays.iter().enumerate() {
        let fail = |message: &str| (array.layer, format!("Density array {index} ({}): {message}", array.material));
        let Some(rule) = technology.physical_rules.density_fill.layers.get(&array.material) else {
            issues.push(fail("no process rule")); continue;
        };
        if array.columns == 0 || array.rows == 0 || array.columns > 32767 || array.rows > 32767
            || ![array.x,array.y,array.tile_width,array.tile_height,array.step_x,array.step_y].iter().all(|v| v.is_finite())
            || array.tile_width <= 0.0 || array.tile_height <= 0.0
            || array.step_x < array.tile_width+rule.fill_spacing_um-1e-9
            || array.step_y < array.tile_height+rule.fill_spacing_um-1e-9
            || density_fill_layer(&array.material, technology) != Some(array.layer) {
            issues.push(fail("invalid dimensions, layer, or spacing")); continue;
        }
        let envelope = &envelopes[index];
        let left = envelope.x-envelope.width/2.0;
        let bottom = envelope.y-envelope.height/2.0;
        if [left,bottom,array.tile_width,array.tile_height,array.step_x,array.step_y].iter()
            .any(|v| (v/grid-(v/grid).round()).abs()>1e-7) {
            issues.push(fail("geometry is off the manufacturing grid"));
        }
        if left < bounds.min_x-1e-9 || bottom < bounds.min_y-1e-9
            || left+envelope.width > bounds.max_x+1e-9 || bottom+envelope.height > bounds.max_y+1e-9 {
            issues.push(fail("tiles escape the die boundary"));
        }
        let halo = rule.circuit_spacing_um.max(rule.ring_keepout_um).max(rule.fill_spacing_um);
        if spatial.query(envelope,halo).into_iter().any(|i|
            density_fill_obstacle(&array.material,envelope,&shapes[i],rule)) {
            issues.push(fail("array envelope enters a circuit keepout"));
        }
        if prior.query(envelope,rule.fill_spacing_um).into_iter().any(|i|
            density_fill_obstacle(&array.material,envelope,&envelopes[i],rule)) {
            issues.push(fail("arrays overlap or violate dummy spacing"));
        }
        prior.insert(index,envelope);
        if let Some(support) = &rule.support_layer {
            let supported = arrays.iter().filter(|a| &a.material == support).any(|base| {
                let dx = (array.x-base.x)/base.step_x;
                let dy = (array.y-base.y)/base.step_y;
                (array.step_x-base.step_x).abs()<1e-9 && (array.step_y-base.step_y).abs()<1e-9
                    && (dx-dx.round()).abs()<1e-7 && (dy-dy.round()).abs()<1e-7
                    && dx >= -1e-9 && dy >= -1e-9
                    && dx+f64::from(array.columns) <= f64::from(base.columns)+1e-9
                    && dy+f64::from(array.rows) <= f64::from(base.rows)+1e-9
                    && array.tile_width <= base.tile_width+1e-9 && array.tile_height <= base.tile_height+1e-9
            });
            if !supported { issues.push(fail("support material does not cover every tile")); }
        }
    }
    if !issues.is_empty() { return issues; }
    for layer in measure(shapes,arrays,bounds,technology).layers {
        if layer.achieved_global_density+1e-9 < layer.minimum_global_density
            || layer.maximum_global_density.is_some_and(|max| layer.achieved_global_density>max+1e-9)
            || layer.underfilled_window_count>0 || layer.overfilled_window_count>0 {
            issues.push((density_fill_layer(&layer.material,technology).unwrap(), format!(
                "{} density outside process limits: global {:.4}, minimum window {:.4}, maximum window {:.4}",
                layer.material,layer.achieved_global_density,layer.achieved_minimum_window_density,layer.achieved_maximum_window_density)));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipped_array_area_matches_expanded_tiles() {
        let array = DensityArray { material: "metal1".into(), layer: PhysicalLayer::Metal(1),
            x: -3.5, y: -5.5, tile_width: 2.0, tile_height: 3.0,
            step_x: 3.2, step_y: 4.2, columns: 7, rows: 9 };
        for offset in -20..40 {
            let bounds = PhysicalBounds { min_x: f64::from(offset)*0.3, max_x: f64::from(offset)*0.3+7.7,
                min_y: -3.0, max_y: 17.25 };
            let mut expected = 0.0;
            for row in 0..array.rows { for col in 0..array.columns {
                let x = array.x + f64::from(col)*array.step_x;
                let y = array.y + f64::from(row)*array.step_y;
                expected += (bounds.max_x.min(x+1.0)-bounds.min_x.max(x-1.0)).max(0.0)
                    * (bounds.max_y.min(y+1.5)-bounds.min_y.max(y-1.5)).max(0.0);
            }}
            assert!((array.area_in(&bounds)-expected).abs()<1e-8);
        }
    }
}
