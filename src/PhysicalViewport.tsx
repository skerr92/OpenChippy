import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { MapControls } from "three/addons/controls/MapControls.js";
import type { PhysicalLayoutIr } from "./types";

type ViewMode = "schematic" | "3d" | "waveform";

type Props = {
  layout: PhysicalLayoutIr | null;
  selectedIds: string[];
  projectName: string;
  technologyName: string;
  onSelect: (id: string | null, additive?: boolean) => void;
  onView: (view: ViewMode) => void;
};

type LayerStyle = {
  label: string;
  color: string;
  elevation: number;
  thickness: number;
};

const BASE_LAYERS: Record<string, LayerStyle> = {
  substrate: { label: "Substrate", color: "#263548", elevation: -.12, thickness: .18 },
  nwell: { label: "N-well", color: "#76599a", elevation: .02, thickness: .07 },
  ndiff: { label: "N diffusion", color: "#38bca8", elevation: .12, thickness: .11 },
  pdiff: { label: "P diffusion", color: "#d276a5", elevation: .12, thickness: .11 },
  poly: { label: "Poly", color: "#e47f3f", elevation: .25, thickness: .15 },
  contact: { label: "Contact", color: "#e7c46a", elevation: .34, thickness: .36 },
};

const METAL_COLORS = ["#5fa8e7", "#b66ee8", "#ee6fa7", "#79c968", "#e4aa54", "#67c9ce"];

function createLayers(maxMetalLayers: number): Record<string, LayerStyle> {
  const layers = { ...BASE_LAYERS };
  for (let index = 1; index <= maxMetalLayers; index += 1) {
    const elevation = .48 + (index - 1) * .24;
    layers[`metal${index}`] = {
      label: `Metal ${index}`,
      color: METAL_COLORS[(index - 1) % METAL_COLORS.length],
      elevation,
      thickness: .13,
    };
    if (index < maxMetalLayers) {
      layers[`via${index}${index + 1}`] = {
        label: `M${index}–M${index + 1} via`,
        color: "#f1e2a1",
        elevation: elevation + .12,
        thickness: .32,
      };
    }
  }
  return layers;
}

type LayerName = string;

export default function PhysicalViewport({
  layout,
  selectedIds,
  projectName,
  technologyName,
  onSelect,
  onView,
}: Props) {
  const host = useRef<HTMLDivElement>(null);
  const resetView = useRef<() => void>(() => {});
  const selectRef = useRef(onSelect);
  const layers = useMemo(() => createLayers(layout?.maxMetalLayers ?? 5), [layout?.maxMetalLayers]);
  const [visibleLayers, setVisibleLayers] = useState<Set<LayerName>>(
    () => new Set(Object.keys(createLayers(5))),
  );
  selectRef.current = onSelect;

  useEffect(() => {
    setVisibleLayers(new Set(Object.keys(layers)));
  }, [layers]);

  useEffect(() => {
    if (!host.current || !layout) return;
    const container = host.current;
    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#08110e");
    const width = layout.bounds.maxX - layout.bounds.minX;
    const depth = layout.bounds.maxY - layout.bounds.minY;
    const center = new THREE.Vector3(
      (layout.bounds.minX + layout.bounds.maxX) / 2,
      0,
      (layout.bounds.minY + layout.bounds.maxY) / 2,
    );
    const span = Math.max(width, depth, 5);
    const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, .05, span * 12);
    camera.up.set(0, 0, -1);

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);
    const controls = new MapControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = .08;
    controls.screenSpacePanning = true;
    controls.minZoom = .00001;
    controls.maxZoom = 200;

    scene.add(new THREE.HemisphereLight("#d8fff2", "#101713", 2.2));
    const key = new THREE.DirectionalLight("#fff0cf", 3);
    key.position.set(center.x + 5, 12, center.z + 6);
    scene.add(key);

    const geometries: THREE.BufferGeometry[] = [];
    const materials: THREE.Material[] = [];
    const pickables: THREE.Mesh[] = [];
    layout.shapes
      .filter((shape) => visibleLayers.has(shape.layer))
      .forEach((shape) => {
        const layer = layers[shape.layer];
        if (!layer) return;
        const geometry = new THREE.BoxGeometry(
          Math.max(shape.width, .04),
          layer.thickness,
          Math.max(shape.height, .04),
        );
        const selected = shape.componentId ? selectedIds.includes(shape.componentId) : false;
        const material = new THREE.MeshStandardMaterial({
          color: selected ? "#ffd277" : layer.color,
          transparent: shape.layer === "nwell" || shape.layer === "substrate",
          opacity: shape.layer === "nwell" ? .48 : shape.layer === "substrate" ? .9 : 1,
          roughness: .5,
          metalness: shape.layer.startsWith("metal") ? .18 : .04,
        });
        const mesh = new THREE.Mesh(geometry, material);
        mesh.position.set(shape.x, layer.elevation, shape.y);
        if (shape.componentId) {
          mesh.userData.componentId = shape.componentId;
          pickables.push(mesh);
        }
        geometries.push(geometry);
        materials.push(material);
        scene.add(mesh);
      });

    const boundary = new THREE.Box3(
      new THREE.Vector3(layout.bounds.minX, -.24, layout.bounds.minY),
      new THREE.Vector3(
        layout.bounds.maxX,
        .48 + Math.max(layout.maxMetalLayers - 1, 0) * .24 + .2,
        layout.bounds.maxY,
      ),
    );
    scene.add(new THREE.Box3Helper(boundary, new THREE.Color("#3a5b50")));

    const fit = () => {
      const aspect = container.clientWidth / Math.max(container.clientHeight, 1);
      camera.left = -aspect;
      camera.right = aspect;
      camera.top = 1;
      camera.bottom = -1;
      camera.zoom = Math.min(
        (2 * aspect) / Math.max(width * 1.25, .1),
        2 / Math.max(depth * 1.25, .1),
      );
      camera.updateProjectionMatrix();
    };
    resetView.current = () => {
      controls.target.copy(center);
      camera.position.set(center.x, span * 2, center.z + .001);
      camera.up.set(0, 0, -1);
      camera.lookAt(center);
      fit();
      controls.update();
    };
    resetView.current();

    const raycaster = new THREE.Raycaster();
    const pointer = new THREE.Vector2();
    let pointerStart = { x: 0, y: 0 };
    const pointerDown = (event: PointerEvent) => {
      pointerStart = { x: event.clientX, y: event.clientY };
    };
    const pointerUp = (event: PointerEvent) => {
      if (Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 4) return;
      const bounds = renderer.domElement.getBoundingClientRect();
      pointer.x = ((event.clientX - bounds.left) / bounds.width) * 2 - 1;
      pointer.y = -((event.clientY - bounds.top) / bounds.height) * 2 + 1;
      raycaster.setFromCamera(pointer, camera);
      const hit = raycaster.intersectObjects(pickables)[0];
      selectRef.current(hit?.object.userData.componentId ?? null, event.shiftKey);
    };
    renderer.domElement.addEventListener("pointerdown", pointerDown);
    renderer.domElement.addEventListener("pointerup", pointerUp);

    const resize = () => {
      renderer.setSize(container.clientWidth, container.clientHeight, false);
      fit();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(container);
    resize();
    let frame = 0;
    const draw = () => {
      frame = requestAnimationFrame(draw);
      controls.update();
      renderer.render(scene, camera);
    };
    draw();

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      renderer.domElement.removeEventListener("pointerdown", pointerDown);
      renderer.domElement.removeEventListener("pointerup", pointerUp);
      controls.dispose();
      geometries.forEach((geometry) => geometry.dispose());
      materials.forEach((material) => material.dispose());
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [layout, selectedIds, visibleLayers, layers]);

  const toggleLayer = (layer: LayerName) => {
    setVisibleLayers((current) => {
      const next = new Set(current);
      if (next.has(layer)) next.delete(layer);
      else next.add(layer);
      return next;
    });
  };

  return (
    <div className="physical-workspace">
      <aside className="physical-sidebar">
        <p className="eyebrow">Physical layout</p>
        <h2>{projectName}</h2>
        <small>{technologyName}</small>
        <p className="physical-sidebar-section">Views</p>
        <div className="physical-view-nav">
          <button onClick={() => onView("schematic")}>2D Schematic</button>
          <button className="active">3D Layout</button>
          <button onClick={() => onView("waveform")}>Waveforms</button>
        </div>
        <button className="physical-reset" onClick={() => resetView.current()}>Top view / Fit</button>
        <p className="physical-sidebar-section">Rust physical IR</p>
        <div className="physical-summary">
          <strong>Format v{layout?.formatVersion ?? "…"}</strong>
          <span>{layout ? `${layout.devices.length} MOS` : "Generating…"}</span>
          <span>{layout ? `${layout.nets.length} nets · ${layout.pins.length} pins` : ""}</span>
          <span>{layout ? `${layout.maxMetalLayers} routing metals` : ""}</span>
        </div>
        <p className="physical-sidebar-section">Layers</p>
        <div className="physical-layers">
          {Object.entries(layers).map(([name, layer]) => (
            <label key={name}>
              <input
                type="checkbox"
                checked={visibleLayers.has(name)}
                onChange={() => toggleLayer(name)}
              />
              <i style={{ background: layer.color }} />
              {layer.label}
            </label>
          ))}
        </div>
        <p className="physical-help">Drag to pan · Right-drag to rotate · Wheel or pinch to zoom · Top view / Fit resets bounds</p>
      </aside>
      <div className="physical-canvas" ref={host}>
        {!layout && <div className="physical-loading">Synthesizing compact layout…</div>}
      </div>
    </div>
  );
}
