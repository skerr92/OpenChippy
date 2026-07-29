import pya


if "input" not in globals():
    raise RuntimeError("pass -rd input=/path/to/layout.gds")

layout = pya.Layout()
layout.read(input)

top_cells = list(layout.top_cells())
print(f"dbu={layout.dbu}")
print(f"cells={layout.cells()}")
print(f"top_cells={len(top_cells)}")

for cell in layout.each_cell():
    instances = sum(1 for _ in cell.each_inst())
    direct_shapes = sum(cell.shapes(index).size() for index in layout.layer_indexes())
    print(
        f"cell={cell.name} instances={instances} "
        f"direct_shapes={direct_shapes} bbox={cell.bbox()}"
    )

for layer_index in layout.layer_indexes():
    info = layout.get_info(layer_index)
    direct_count = sum(
        cell.shapes(layer_index).size() for cell in layout.each_cell()
    )
    print(f"layer={info.layer}/{info.datatype} direct_shapes={direct_count}")

if len(top_cells) == 1:
    top = top_cells[0]
    recursive_count = sum(
        sum(1 for _ in top.begin_shapes_rec(layer_index))
        for layer_index in layout.layer_indexes()
    )
    print(f"top={top.name}")
    print(f"top_bbox={top.bbox()}")
    print(f"top_recursive_shapes={recursive_count}")

if "roundtrip" in globals() and roundtrip:
    layout.write(roundtrip)
    reread = pya.Layout()
    reread.read(roundtrip)
    print(
        f"roundtrip_cells={reread.cells()} "
        f"roundtrip_top_cells={len(list(reread.top_cells()))}"
    )
