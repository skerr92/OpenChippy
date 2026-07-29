import os
import pya

path = os.environ["OPENCHIPPY_LEF"]
layout = pya.Layout()
layout.read(path)
tops = layout.top_cells()
if len(tops) != 1:
    raise RuntimeError(f"expected one top macro, found {len(tops)}")
top = tops[0]
box = top.bbox()
shape_count = 0
text_values = []
for layer_index in layout.layer_indexes():
    for shape in top.shapes(layer_index).each():
        shape_count += 1
        if shape.is_text():
            text_values.append(shape.text.string)
print(
    f"KLayout parsed {path}: top={top.name}, "
    f"bounds={box.width() * layout.dbu:.6f}x{box.height() * layout.dbu:.6f} um, "
    f"shapes={shape_count}, labels={','.join(sorted(text_values)) or 'none'}"
)
