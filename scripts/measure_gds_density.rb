# Usage:
#   klayout -b -r scripts/measure_gds_density.rb -rd input=design.gds

layout = RBA::Layout.new
layout.read($input)
cell = layout.top_cell
bounds = cell.bbox
die_area = bounds.width * bounds.height

[
  ["active", [[22, 0], [22, 4]]],
  ["poly", [[30, 0], [30, 4]]],
  ["metal1", [[34, 0], [34, 4]]],
  ["metal2", [[36, 0], [36, 4]]],
  ["metal3", [[42, 0], [42, 4]]],
  ["metal4", [[46, 0], [46, 4]]],
  ["metal5", [[81, 0], [81, 4]]],
  ["top_metal", [[53, 0], [53, 4]]]
].each do |name, pairs|
  region = RBA::Region.new
  pairs.each do |layer, datatype|
    index = layout.find_layer(layer, datatype)
    region += RBA::Region.new(cell.begin_shapes_rec(index)) unless index.nil?
  end
  density = die_area.zero? ? 0.0 : region.merged.area.to_f / die_area
  puts format("%-10s %.6f", name, density)
end
