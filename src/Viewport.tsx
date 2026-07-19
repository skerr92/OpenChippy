import { useEffect, useRef } from "react";
import * as THREE from "three";
import { MapControls } from "three/addons/controls/MapControls.js";
import { terminalPosition } from "./SchematicViewport";
import type { Component, Wire } from "./types";

type Props = {
  components: Component[];
  wires: Wire[];
  selectedIds: string[];
  onSelect: (id: string | null, additive?: boolean) => void;
};

const LAYERS = {
  substrate: { color: "#263548", y: -.12, height: .22 },
  nwell: { color: "#76599a", y: .035, height: .08 },
  ndiff: { color: "#38bca8", y: .11, height: .12 },
  pdiff: { color: "#d276a5", y: .11, height: .12 },
  poly: { color: "#e47f3f", y: .24, height: .16 },
  metal1: { color: "#5fa8e7", y: .48, height: .13 },
  metal2: { color: "#b66ee8", y: .72, height: .13 },
  contact: { color: "#e7c46a", y: .34, height: .38 },
  via12: { color: "#f1e2a1", y: .60, height: .32 },
} as const;

type MetalLayer = "metal1" | "metal2";
type Route = {
  wire: Wire;
  fromComponent: Component;
  toComponent?: Component;
  points: THREE.Vector2[];
};

const terminalKey = (componentId: string, terminal: string) => `${componentId}:${terminal}`;

const segmentsIntersect = (
  a1: THREE.Vector2,
  a2: THREE.Vector2,
  b1: THREE.Vector2,
  b2: THREE.Vector2,
) => {
  const epsilon = .0001;
  const between = (value: number, edge1: number, edge2: number) =>
    value >= Math.min(edge1, edge2) - epsilon && value <= Math.max(edge1, edge2) + epsilon;
  const aHorizontal = Math.abs(a1.y - a2.y) < epsilon;
  const bHorizontal = Math.abs(b1.y - b2.y) < epsilon;

  if (aHorizontal !== bHorizontal) {
    const horizontal1 = aHorizontal ? a1 : b1;
    const horizontal2 = aHorizontal ? a2 : b2;
    const vertical1 = aHorizontal ? b1 : a1;
    const vertical2 = aHorizontal ? b2 : a2;
    return between(vertical1.x, horizontal1.x, horizontal2.x)
      && between(horizontal1.y, vertical1.y, vertical2.y);
  }
  if (aHorizontal) {
    return Math.abs(a1.y - b1.y) < epsilon
      && Math.max(Math.min(a1.x, a2.x), Math.min(b1.x, b2.x))
        <= Math.min(Math.max(a1.x, a2.x), Math.max(b1.x, b2.x)) + epsilon;
  }
  return Math.abs(a1.x - b1.x) < epsilon
    && Math.max(Math.min(a1.y, a2.y), Math.min(b1.y, b2.y))
      <= Math.min(Math.max(a1.y, a2.y), Math.max(b1.y, b2.y)) + epsilon;
};

export default function Viewport({ components, wires, selectedIds, onSelect }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const resetViewRef = useRef<() => void>(() => {});
  const selectRef = useRef(onSelect);
  selectRef.current = onSelect;

  useEffect(() => {
    if (!host.current) return;
    const container = host.current;
    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#08110e");

    const xs = components.map((component) => component.position.x);
    const zs = components.map((component) => component.position.y);
    const minX = (xs.length ? Math.min(...xs) : -3) - 2;
    const maxX = (xs.length ? Math.max(...xs) : 3) + 2;
    const minZ = (zs.length ? Math.min(...zs) : -2) - 2;
    const maxZ = (zs.length ? Math.max(...zs) : 2) + 2;
    const width = maxX - minX;
    const depth = maxZ - minZ;
    const center = new THREE.Vector3((minX + maxX) / 2, 0, (minZ + maxZ) / 2);
    const span = Math.max(width, depth, 6);

    const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, .05, span * 10);
    camera.up.set(0, 0, -1);
    camera.position.set(center.x, span * 2, center.z + .001);
    camera.lookAt(center.x, 0, center.z);

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);
    const controls = new MapControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = .08;
    controls.screenSpacePanning = true;
    controls.target.set(center.x, .18, center.z);
    controls.minZoom = .02;
    controls.maxZoom = 20;

    scene.add(new THREE.HemisphereLight("#d8fff2", "#101713", 2.2));
    const key = new THREE.DirectionalLight("#fff0cf", 3);
    key.position.set(center.x + 5, 12, center.z + 6);
    scene.add(key);

    const pickables: THREE.Mesh[] = [];
    const geometries: THREE.BufferGeometry[] = [];
    const materials: THREE.Material[] = [];
    const componentById = new Map(components.map((component) => [component.id, component]));

    const materialFor = (color: string, opacity = 1) => {
      const material = new THREE.MeshStandardMaterial({
        color,
        transparent: opacity < 1,
        opacity,
        roughness: .5,
        metalness: .08,
      });
      materials.push(material);
      return material;
    };

    const addRect = (
      sizeX: number,
      sizeZ: number,
      x: number,
      z: number,
      layer: keyof typeof LAYERS,
      componentId?: string,
      opacity = 1,
    ) => {
      const spec = LAYERS[layer];
      const geometry = new THREE.BoxGeometry(Math.max(sizeX, .05), spec.height, Math.max(sizeZ, .05));
      const selected = componentId ? selectedIds.includes(componentId) : false;
      const material = materialFor(selected ? "#ffd277" : spec.color, opacity);
      const mesh = new THREE.Mesh(geometry, material);
      mesh.position.set(x, spec.y, z);
      geometries.push(geometry);
      if (componentId) {
        mesh.userData.componentId = componentId;
        pickables.push(mesh);
      }
      scene.add(mesh);
      return mesh;
    };

    // The cell boundary is inferred from placed devices, not a fixed transistor scale.
    addRect(width, depth, center.x, center.z, "substrate", undefined, .92);
    const boundary = new THREE.Box3(
      new THREE.Vector3(minX, -.25, minZ),
      new THREE.Vector3(maxX, .6, maxZ),
    );
    const boundaryHelper = new THREE.Box3Helper(boundary, new THREE.Color("#3a5b50"));
    scene.add(boundaryHelper);

    const pmos = components.filter((component) => component.kind === "pmos");
    if (pmos.length) {
      const wellMinX = Math.min(...pmos.map((item) => item.position.x)) - 1.6;
      const wellMaxX = Math.max(...pmos.map((item) => item.position.x)) + 1.6;
      const wellMinZ = Math.min(...pmos.map((item) => item.position.y)) - 1.4;
      const wellMaxZ = Math.max(...pmos.map((item) => item.position.y)) + 1.4;
      addRect(wellMaxX - wellMinX, wellMaxZ - wellMinZ, (wellMinX + wellMaxX) / 2, (wellMinZ + wellMaxZ) / 2, "nwell", undefined, .48);
    }

    components.forEach((component) => {
      const x = component.position.x;
      const z = component.position.y;
      if (component.kind === "nmos" || component.kind === "pmos") {
        const diffusion = component.kind === "nmos" ? "ndiff" : "pdiff";
        const diffusionShape = addRect(2.5, 1.15, x, z, diffusion, component.id);
        diffusionShape.rotation.y = component.rotation;
        const poly = addRect(.28, 1.75, x, z, "poly", component.id);
        poly.rotation.y = component.rotation;
      } else if (component.kind === "vdd" || component.kind === "gnd") {
        const [, railZ] = terminalPosition(component, "out");
        addRect(width - .8, .5, center.x, railZ, "metal1", component.id);
      } else if (component.kind === "input" || component.kind === "output") {
        const terminal = component.kind === "input" ? "out" : "in";
        const [pinX, pinZ] = terminalPosition(component, terminal);
        addRect(1.0, .65, pinX, pinZ, "metal1", component.id);
      } else if (component.kind === "resistor") {
        addRect(2.6, .3, x, z, "poly", component.id);
      }
    });

    const routes: Route[] = [];
    wires.forEach((wire) => {
      const fromComponent = componentById.get(wire.from.componentId);
      const toComponent = wire.to ? componentById.get(wire.to.componentId) : undefined;
      if (!fromComponent || (wire.to && !toComponent)) return;
      const [x1, z1] = terminalPosition(fromComponent, wire.from.terminal);
      const [x2, z2] = wire.to && toComponent
        ? terminalPosition(toComponent, wire.to.terminal)
        : [wire.end?.x ?? x1, wire.end?.y ?? z1];
      const waypoints = wire.waypoints ?? [];
      const points = [new THREE.Vector2(x1, z1)];
      if (waypoints.length) {
        waypoints.forEach((waypoint) => {
          const previous = points[points.length - 1];
          points.push(new THREE.Vector2(waypoint.x, previous.y));
          points.push(new THREE.Vector2(waypoint.x, waypoint.y));
        });
        if (wire.to) {
          const previous = points[points.length - 1];
          points.push(new THREE.Vector2(x2, previous.y));
          points.push(new THREE.Vector2(x2, z2));
        }
      } else {
        const routeX = wire.routeX ?? (x1 + x2) / 2;
        points.push(
          new THREE.Vector2(routeX, z1),
          new THREE.Vector2(routeX, z2),
          new THREE.Vector2(x2, z2),
        );
      }
      routes.push({
        wire,
        fromComponent,
        toComponent,
        points,
      });
    });

    // Infer nets from shared terminals and equal-name net labels. Assign an
    // entire net to one routing layer so junction branches never create vias.
    const parents = routes.map((_, index) => index);
    const find = (index: number): number => {
      if (parents[index] !== index) parents[index] = find(parents[index]);
      return parents[index];
    };
    const union = (left: number, right: number) => {
      const leftRoot = find(left);
      const rightRoot = find(right);
      if (leftRoot !== rightRoot) parents[rightRoot] = leftRoot;
    };
    const firstRouteAtTerminal = new Map<string, number>();
    const firstRouteAtLabel = new Map<string, number>();
    routes.forEach((route, routeIndex) => {
      [route.wire.from, route.wire.to].filter((terminal) => terminal !== null).forEach((terminal) => {
        const key = terminalKey(terminal.componentId, terminal.terminal);
        const previous = firstRouteAtTerminal.get(key);
        if (previous === undefined) firstRouteAtTerminal.set(key, routeIndex);
        else union(routeIndex, previous);

        const component = componentById.get(terminal.componentId);
        if (component?.kind === "net_label") {
          const labelRoute = firstRouteAtLabel.get(component.name);
          if (labelRoute === undefined) firstRouteAtLabel.set(component.name, routeIndex);
          else union(routeIndex, labelRoute);
        }
      });
    });

    const conflicts = new Map<number, Set<number>>();
    const addConflict = (left: number, right: number) => {
      if (!conflicts.has(left)) conflicts.set(left, new Set());
      if (!conflicts.has(right)) conflicts.set(right, new Set());
      conflicts.get(left)?.add(right);
      conflicts.get(right)?.add(left);
    };
    for (let leftIndex = 0; leftIndex < routes.length; leftIndex += 1) {
      for (let rightIndex = leftIndex + 1; rightIndex < routes.length; rightIndex += 1) {
        const leftNet = find(leftIndex);
        const rightNet = find(rightIndex);
        if (leftNet === rightNet) continue;
        const left = routes[leftIndex].points;
        const right = routes[rightIndex].points;
        const intersects = left.slice(0, -1).some((start, leftSegment) =>
          right.slice(0, -1).some((otherStart, rightSegment) =>
            segmentsIntersect(start, left[leftSegment + 1], otherStart, right[rightSegment + 1])));
        if (intersects) addConflict(leftNet, rightNet);
      }
    }

    const layerByNet = new Map<number, MetalLayer>();
    [...new Set(routes.map((_, index) => find(index)))]
      .sort((left, right) => (conflicts.get(right)?.size ?? 0) - (conflicts.get(left)?.size ?? 0))
      .forEach((net) => {
        const neighbors = conflicts.get(net) ?? new Set();
        const m1Conflicts = [...neighbors].filter((neighbor) => layerByNet.get(neighbor) === "metal1").length;
        const m2Conflicts = [...neighbors].filter((neighbor) => layerByNet.get(neighbor) === "metal2").length;
        layerByNet.set(net, m1Conflicts <= m2Conflicts ? "metal1" : "metal2");
      });

    const addMetalSegment = (start: THREE.Vector2, end: THREE.Vector2, layer: MetalLayer) => {
      const horizontal = Math.abs(end.x - start.x) >= Math.abs(end.y - start.y);
      addRect(
        horizontal ? Math.abs(end.x - start.x) + .28 : .28,
        horizontal ? .28 : Math.abs(end.y - start.y) + .28,
        (start.x + end.x) / 2,
        (start.y + end.y) / 2,
        layer,
      );
    };
    routes.forEach((route, routeIndex) => {
      const layer = layerByNet.get(find(routeIndex)) ?? "metal1";
      for (let index = 0; index < route.points.length - 1; index += 1) {
        addMetalSegment(route.points[index], route.points[index + 1], layer);
      }
      const needsContact = (component: Component) =>
        component.kind === "nmos"
        || component.kind === "pmos"
        || component.kind === "resistor";
      const needsPhysicalConnection = (component: Component) =>
        component.kind !== "junction" && component.kind !== "net_label";
      const [fromPoint, toPoint] = [route.points[0], route.points[route.points.length - 1]];
      if (needsContact(route.fromComponent)) {
        addRect(.3, .3, fromPoint.x, fromPoint.y, "contact", route.fromComponent.id);
      }
      if (route.toComponent && needsContact(route.toComponent)) {
        addRect(.3, .3, toPoint.x, toPoint.y, "contact", route.toComponent.id);
      }
      if (layer === "metal2" && needsPhysicalConnection(route.fromComponent)) {
        addRect(.34, .34, fromPoint.x, fromPoint.y, "via12", route.fromComponent.id);
      }
      if (layer === "metal2" && route.toComponent && needsPhysicalConnection(route.toComponent)) {
        addRect(.34, .34, toPoint.x, toPoint.y, "via12", route.toComponent.id);
      }
    });

    // Fit against the completed physical geometry. Route detours, terminal
    // offsets, rails, and wells can all extend beyond schematic component origins.
    const physicalBounds = new THREE.Box3().setFromObject(scene);
    const physicalCenter = physicalBounds.getCenter(new THREE.Vector3());
    const physicalSize = physicalBounds.getSize(new THREE.Vector3());
    controls.target.set(physicalCenter.x, physicalCenter.y, physicalCenter.z);
    camera.position.set(
      physicalCenter.x,
      physicalCenter.y + Math.max(physicalSize.x, physicalSize.z, 4) * 2,
      physicalCenter.z + .001,
    );
    camera.up.set(0, 0, -1);
    camera.lookAt(physicalCenter);
    controls.update();
    const fitTopView = () => {
      const aspect = container.clientWidth / Math.max(container.clientHeight, 1);
      camera.left = -aspect;
      camera.right = aspect;
      camera.top = 1;
      camera.bottom = -1;
      camera.zoom = Math.min(
        (2 * aspect) / Math.max(physicalSize.x * 1.3, .1),
        2 / Math.max(physicalSize.z * 1.3, .1),
      );
      camera.updateProjectionMatrix();
    };
    resetViewRef.current = () => {
      controls.target.copy(physicalCenter);
      camera.position.set(
        physicalCenter.x,
        physicalCenter.y + Math.max(physicalSize.x, physicalSize.z, 4) * 2,
        physicalCenter.z + .001,
      );
      camera.up.set(0, 0, -1);
      camera.lookAt(physicalCenter);
      fitTopView();
      controls.update();
    };

    const raycaster = new THREE.Raycaster();
    const pointer = new THREE.Vector2();
    let pointerStart = { x: 0, y: 0 };
    const handlePointerDown = (event: PointerEvent) => {
      pointerStart = { x: event.clientX, y: event.clientY };
    };
    const handlePointerUp = (event: PointerEvent) => {
      if (Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) > 4) return;
      const bounds = renderer.domElement.getBoundingClientRect();
      pointer.x = ((event.clientX - bounds.left) / bounds.width) * 2 - 1;
      pointer.y = -((event.clientY - bounds.top) / bounds.height) * 2 + 1;
      raycaster.setFromCamera(pointer, camera);
      const hit = raycaster.intersectObjects(pickables)[0];
      selectRef.current(hit?.object.userData.componentId ?? null, event.shiftKey);
    };
    renderer.domElement.addEventListener("pointerdown", handlePointerDown);
    renderer.domElement.addEventListener("pointerup", handlePointerUp);

    const handleArrowPan = (event: KeyboardEvent) => {
      if (!event.key.startsWith("Arrow")) return;
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
      event.preventDefault();
      const visibleHeight = 2 / camera.zoom;
      const step = visibleHeight * .06;
      const delta = new THREE.Vector3(
        event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0,
        0,
        event.key === "ArrowUp" ? -step : event.key === "ArrowDown" ? step : 0,
      );
      camera.position.add(delta);
      controls.target.add(delta);
      controls.update();
    };
    window.addEventListener("keydown", handleArrowPan);

    const resize = () => {
      renderer.setSize(container.clientWidth, container.clientHeight, false);
      fitTopView();
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
      controls.dispose();
      resetViewRef.current = () => {};
      renderer.domElement.removeEventListener("pointerdown", handlePointerDown);
      renderer.domElement.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("keydown", handleArrowPan);
      renderer.dispose();
      geometries.forEach((geometry) => geometry.dispose());
      materials.forEach((material) => material.dispose());
      boundaryHelper.geometry.dispose();
      if (Array.isArray(boundaryHelper.material)) {
        boundaryHelper.material.forEach((material) => material.dispose());
      } else {
        boundaryHelper.material.dispose();
      }
      container.removeChild(renderer.domElement);
    };
  }, [components, wires, selectedIds]);

  return (
    <div className="layout-viewport">
      <div className="viewport" ref={host} aria-label="3D standard-cell layout viewport" />
      <button className="reset-layout-view" onClick={() => resetViewRef.current()}>Top view</button>
      <div className="layer-legend">
        <span className="ndiff">N diffusion</span>
        <span className="pdiff">P diffusion</span>
        <span className="poly">Poly</span>
        <span className="metal1">Metal 1</span>
        <span className="metal2">Metal 2</span>
        <span className="contact">Device contact</span>
        <span className="via12">M1–M2 via</span>
        <span className="nwell">N-well</span>
      </div>
    </div>
  );
}
