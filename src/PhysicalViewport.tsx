import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { MapControls } from "three/addons/controls/MapControls.js";
import type { PhysicalBuildProgress, PhysicalBuildReport, PhysicalDrcDiagnostic, PhysicalDrcReport, PhysicalLayoutIr } from "./types";

type ViewMode = "schematic" | "3d" | "waveform";

type Props = {
  layout: PhysicalLayoutIr | null;
  drc: PhysicalDrcReport | null;
  buildReport: PhysicalBuildReport | null;
  buildProgress: PhysicalBuildProgress | null;
  buildStartedAt: number | null;
  buildError: string | null;
  selectedIds: string[];
  projectName: string;
  technologyName: string;
  onSelect: (id: string | null, additive?: boolean) => void;
  onSaveDrc: () => void;
  onSaveLayout: () => void;
  onExportGds: () => void;
  onExportLef: () => void;
  onRetry: () => void;
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
  pwell: { label: "P-well", color: "#436b76", elevation: -.01, thickness: .07 },
  nwell: { label: "N-well", color: "#76599a", elevation: .02, thickness: .07 },
  ndiff: { label: "N diffusion", color: "#38bca8", elevation: .12, thickness: .11 },
  pdiff: { label: "P diffusion", color: "#d276a5", elevation: .12, thickness: .11 },
  poly: { label: "Poly", color: "#e47f3f", elevation: .25, thickness: .15 },
  contact: { label: "Contact", color: "#e7c46a", elevation: .34, thickness: .36 },
};

const METAL_COLORS = ["#5fa8e7", "#b66ee8", "#ee6fa7", "#79c968", "#e4aa54", "#67c9ce"];

function createLayers(maxMetalLayers: number, renderedMetalLayers = maxMetalLayers): Record<string, LayerStyle> {
  const layers = { ...BASE_LAYERS };
  for (let index = 1; index <= renderedMetalLayers; index += 1) {
    const elevation = .48 + (index - 1) * .24;
    layers[`metal${index}`] = {
      label: index > maxMetalLayers ? "Top-metal fill" : `Metal ${index}`,
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
type CameraSnapshot = {
  position: THREE.Vector3;
  target: THREE.Vector3;
  up: THREE.Vector3;
  zoom: number;
};

export default function PhysicalViewport({
  layout,
  drc,
  buildReport,
  buildProgress,
  buildStartedAt,
  buildError,
  selectedIds,
  projectName,
  technologyName,
  onSelect,
  onSaveDrc,
  onSaveLayout,
  onExportGds,
  onExportLef,
  onRetry,
  onView,
}: Props) {
  const host = useRef<HTMLDivElement>(null);
  const fitView = useRef<() => void>(() => {});
  const orientView = useRef<(orientation: "top" | "iso" | "front" | "right") => void>(() => {});
  const cameraSnapshot = useRef<CameraSnapshot | null>(null);
  const cameraLayout = useRef<PhysicalLayoutIr | null>(null);
  const selectRef = useRef(onSelect);
  const renderedMetalLayers = useMemo(() => {
    const routable = layout?.maxMetalLayers ?? 5;
    return layout?.shapes.reduce((maximum, shape) => {
      const match = /^metal(\d+)$/.exec(shape.layer);
      return match ? Math.max(maximum, Number(match[1])) : maximum;
    }, routable) ?? routable;
  }, [layout]);
  const layers = useMemo(
    () => createLayers(layout?.maxMetalLayers ?? 5, renderedMetalLayers),
    [layout?.maxMetalLayers, renderedMetalLayers],
  );
  const [visibleLayers, setVisibleLayers] = useState<Set<LayerName>>(
    () => new Set(Object.keys(createLayers(5))),
  );
  const [selectedDiagnostic, setSelectedDiagnostic] = useState<number | null>(null);
  const [clockNow, setClockNow] = useState(Date.now());
  useEffect(() => {
    if (layout || buildError || buildStartedAt === null) return;
    setClockNow(Date.now());
    const timer = window.setInterval(() => setClockNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [layout, buildError, buildStartedAt]);
  const [drcLayer, setDrcLayer] = useState("all");
  const [drcRule, setDrcRule] = useState("all");
  const [drcSeverity, setDrcSeverity] = useState("all");
  const [drcNet, setDrcNet] = useState("all");
  const [showBlockRegions, setShowBlockRegions] = useState(true);
  const [showDummyFill, setShowDummyFill] = useState(true);
  const [soloDummyFill, setSoloDummyFill] = useState(false);
  const dummyFillByLayer = useMemo(() => {
    const counts: Record<string, number> = {};
    layout?.shapes.forEach((shape) => {
      if (shape.purpose === "dummy_fill") counts[shape.layer] = (counts[shape.layer] ?? 0) + 1;
    });
    return counts;
  }, [layout]);
  const dummyFillCount = Object.values(dummyFillByLayer).reduce((sum, count) => sum + count, 0);
  selectRef.current = onSelect;

  useEffect(() => {
    setVisibleLayers(new Set(Object.keys(layers)));
    setSelectedDiagnostic(null);
  }, [layers]);

  const selectedDrc = selectedDiagnostic === null ? null : drc?.diagnostics[selectedDiagnostic] ?? null;

  useEffect(() => {
    if (!host.current || !layout) return;
    const sameLayout = cameraLayout.current === layout;
    cameraLayout.current = layout;
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
    const highlightedIndices = new Set(selectedDrc?.shapeIndices ?? []);
    const highlightedBox = new THREE.Box3();
    layout.shapes.forEach((shape, shapeIndex) => {
        if (!visibleLayers.has(shape.layer)) return;
        const dummyFill = shape.purpose === "dummy_fill";
        if (dummyFill ? !showDummyFill : soloDummyFill) return;
        const layer = layers[shape.layer];
        if (!layer) return;
        const geometry = new THREE.BoxGeometry(
          Math.max(shape.width, .04),
          layer.thickness,
          Math.max(shape.height, .04),
        );
        const selected = shape.componentId ? selectedIds.includes(shape.componentId) : false;
        const violation = highlightedIndices.has(shapeIndex);
        const baseColor = dummyFill
          ? new THREE.Color(layer.color).lerp(new THREE.Color("#fff2a8"), .3)
          : new THREE.Color(layer.color);
        const material = new THREE.MeshStandardMaterial({
          color: violation ? "#ff5252" : selected ? "#ffd277" : baseColor,
          emissive: violation ? "#7a0909" : dummyFill ? baseColor : "#000000",
          emissiveIntensity: violation ? 1.8 : dummyFill ? .18 : 0,
          transparent: shape.layer === "nwell" || shape.layer === "substrate",
          opacity: shape.layer === "nwell" ? .48 : shape.layer === "substrate" ? .9 : 1,
          roughness: .5,
          metalness: shape.layer.startsWith("metal") ? .18 : .04,
        });
        const mesh = new THREE.Mesh(geometry, material);
        mesh.position.set(shape.x, layer.elevation, shape.y);
        if (violation) highlightedBox.expandByObject(mesh);
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
    if (showBlockRegions && layout.physicalBlocks.length) {
      const colors = ["#56d6b1", "#e6ba58", "#7ca8ff", "#df7fc0"];
      layout.physicalBlocks.forEach((block, index) => {
        const region = block.bounds;
        const regionBox = new THREE.Box3(
          new THREE.Vector3(region.minX, .36, region.minY),
          new THREE.Vector3(region.maxX, .42, region.maxY),
        );
        scene.add(new THREE.Box3Helper(regionBox, new THREE.Color(colors[index % colors.length])));
      });
    }

    const updateProjection = () => {
      const aspect = container.clientWidth / Math.max(container.clientHeight, 1);
      camera.left = -aspect;
      camera.right = aspect;
      camera.top = 1;
      camera.bottom = -1;
      camera.updateProjectionMatrix();
    };
    const fit = () => {
      const aspect = container.clientWidth / Math.max(container.clientHeight, 1);
      camera.zoom = Math.min(
        (2 * aspect) / Math.max(width * 1.25, .1),
        2 / Math.max(depth * 1.25, .1),
      );
      camera.updateProjectionMatrix();
    };
    const rememberCamera = () => {
      cameraSnapshot.current = {
        position: camera.position.clone(),
        target: controls.target.clone(),
        up: camera.up.clone(),
        zoom: camera.zoom,
      };
    };
    const setOrientation = (orientation: "top" | "iso" | "front" | "right") => {
      controls.target.copy(center);
      if (orientation === "top") {
        camera.position.set(center.x, span * 2, center.z + .001);
        camera.up.set(0, 0, -1);
      } else if (orientation === "front") {
        camera.position.set(center.x, span * .35, center.z + span * 2);
        camera.up.set(0, 1, 0);
      } else if (orientation === "right") {
        camera.position.set(center.x + span * 2, span * .35, center.z);
        camera.up.set(0, 1, 0);
      } else {
        camera.position.set(center.x + span * 1.35, span * 1.5, center.z + span * 1.35);
        camera.up.set(0, 1, 0);
      }
      camera.lookAt(center);
      controls.update();
      rememberCamera();
    };
    orientView.current = setOrientation;
    fitView.current = () => {
      fit();
      controls.update();
      rememberCamera();
    };
    updateProjection();
    if (sameLayout && cameraSnapshot.current) {
      camera.position.copy(cameraSnapshot.current.position);
      controls.target.copy(cameraSnapshot.current.target);
      camera.up.copy(cameraSnapshot.current.up);
      camera.zoom = cameraSnapshot.current.zoom;
      camera.updateProjectionMatrix();
      controls.update();
    } else {
      setOrientation("top");
      fitView.current();
    }
    if (!highlightedBox.isEmpty()) {
      const violationCenter = highlightedBox.getCenter(new THREE.Vector3());
      const violationSize = highlightedBox.getSize(new THREE.Vector3());
      const violationSpan = Math.max(violationSize.x, violationSize.z, .5);
      controls.target.copy(violationCenter);
      camera.position.set(violationCenter.x, span * 2, violationCenter.z + .001);
      camera.lookAt(violationCenter);
      const aspect = container.clientWidth / Math.max(container.clientHeight, 1);
      camera.zoom = Math.min((2 * aspect) / (violationSpan * 1.8), 2 / (violationSpan * 1.8));
      camera.updateProjectionMatrix();
      controls.update();
      rememberCamera();
    }
    controls.addEventListener("change", rememberCamera);

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
      updateProjection();
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
      controls.removeEventListener("change", rememberCamera);
      controls.dispose();
      geometries.forEach((geometry) => geometry.dispose());
      materials.forEach((material) => material.dispose());
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [layout, selectedIds, visibleLayers, layers, selectedDrc, showBlockRegions, showDummyFill, soloDummyFill]);

  const toggleLayer = (layer: LayerName) => {
    setVisibleLayers((current) => {
      const next = new Set(current);
      if (next.has(layer)) next.delete(layer);
      else next.add(layer);
      return next;
    });
  };

  const showAllLayers = () => setVisibleLayers(new Set(Object.keys(layers)));
  const soloLayer = (layer: LayerName) => setVisibleLayers(new Set([layer]));

  const selectViolation = (index: number, diagnostic: PhysicalDrcDiagnostic) => {
    setSelectedDiagnostic(index);
    if (layers[diagnostic.layer]) setVisibleLayers(new Set([diagnostic.layer]));
  };

  const drcRules = Array.from(new Set(drc?.diagnostics.map((diagnostic) => diagnostic.ruleId) ?? [])).sort();
  const drcLayers = Array.from(new Set(drc?.diagnostics.map((diagnostic) => diagnostic.layer) ?? [])).sort();
  const diagnosticNets = (diagnostic: PhysicalDrcDiagnostic) => Array.from(new Set(
    diagnostic.shapeIndices.flatMap((index) => {
      const net = layout?.shapes[index]?.net;
      return net === null || net === undefined ? [] : [net];
    }),
  ));
  const drcNets = Array.from(new Set((drc?.diagnostics ?? []).flatMap(diagnosticNets))).sort((a, b) => a - b);
  const filteredDiagnostics = (drc?.diagnostics ?? [])
    .map((diagnostic, index) => ({ diagnostic, index }))
    .filter(({ diagnostic }) => drcLayer === "all" || diagnostic.layer === drcLayer)
    .filter(({ diagnostic }) => drcRule === "all" || diagnostic.ruleId === drcRule)
    .filter(({ diagnostic }) => drcSeverity === "all" || diagnostic.severity === drcSeverity)
    .filter(({ diagnostic }) => drcNet === "all" || diagnosticNets(diagnostic).includes(Number(drcNet)));
  const buildStages = [
    { id: "topology", label: "Prepare topology and reusable hierarchy", doneAt: 8 },
    { id: "planning", label: "Plan the floorplan and placement fabric", doneAt: 10 },
    { id: "candidateRouting", label: "Evaluate placement and routing candidates", doneAt: 64 },
    { id: "geometryRefinement", label: "Refine and repair physical geometry", doneAt: 90 },
    { id: "physicalIr", label: "Assemble the physical design", doneAt: 96 },
    { id: "drc", label: "Run categorized physical DRC", doneAt: 100 },
  ];
  const buildPercent = Math.max(0, Math.min(100, buildProgress?.percent ?? 0));
  const elapsedMs = buildStartedAt === null ? 0 : Math.max(0, clockNow - buildStartedAt);
  const estimatedRemainingMs = buildPercent >= 5 && buildPercent < 100
    ? (elapsedMs / buildPercent) * (100 - buildPercent)
    : null;
  const formatDuration = (milliseconds: number) => {
    const totalSeconds = Math.max(0, Math.round(milliseconds / 1000));
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return minutes ? `${minutes}m ${seconds.toString().padStart(2, "0")}s` : `${seconds}s`;
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
        <button
          className="physical-reset"
          onClick={() => setShowBlockRegions((current) => !current)}
          disabled={!layout?.physicalBlocks.length}
        >
          {showBlockRegions ? "Hide" : "Show"} block regions
        </button>
        <p className="physical-sidebar-section">Rust physical IR</p>
        <div className="physical-summary">
          <strong>Format v{layout?.formatVersion ?? "…"}</strong>
          <span>{layout ? `${layout.devices.length} MOS` : "Generating…"}</span>
          <span>{layout ? `${layout.nets.length} nets · ${layout.pins.length} pins` : ""}</span>
          <span>{layout ? `${layout.maxMetalLayers} routing metals` : ""}</span>
          {layout && <span>{dummyFillCount} dummy-fill shapes</span>}
          {layout && Object.entries(dummyFillByLayer).map(([layer, count]) => (
            <span key={`fill-${layer}`}>{layer} fill · {count}</span>
          ))}
          {layout && dummyFillCount === 0 && <strong>No dummy fill in this Physical IR</strong>}
          <span>{layout ? `deck ${layout.technologyFingerprint}` : ""}</span>
          {layout && (() => {
            const candidate = layout.planning.candidates[layout.planning.selectedCandidate];
            const placement = layout.placement.candidates[layout.placement.selectedCandidate];
            const candidateTiming = layout.timing.candidates.find(
              (entry) => entry.candidate === layout.timing.selectedCandidate,
            );
            return candidate ? (
              <>
                <span>{candidate.strategy} plan · {candidate.widthUm.toFixed(2)} × {candidate.heightUm.toFixed(2)} µm</span>
                <span>{(candidate.deviceDensity * 100).toFixed(1)}% density · {(candidate.estimatedRoutingUtilization * 100).toFixed(1)}% route estimate</span>
                <span>{layout.planning.candidates.length} candidates · {candidate.growthPasses} growth passes</span>
                <span>{layout.planning.binColumns} × {layout.planning.binRows} coarse routing bins</span>
                {placement && (
                  <>
                    <span>{placement.strategy} placement · {placement.legal ? "legal" : "illegal"}</span>
                    <span>{placement.estimatedWireLengthUm.toFixed(1)} µm HPWL · {(placement.peakBinUtilization * 100).toFixed(1)}% peak bin</span>
                    <span>{placement.diffusionSharingPairs} diffusion-sharing pairs</span>
                    <span>{placement.reservedDeviceShapes} reserved device shapes · {placement.occupancyRetries} occupancy retries</span>
                    {placement.blockRegions.length > 0 && (
                      <>
                        <span>{placement.blockRegions.length} staged block regions</span>
                        <span>private → shared → external → power routing</span>
                        <span>local M1 rails · deferred M2 power stitch</span>
                      </>
                    )}
                  </>
                )}
                <span>{layout.globalRouting.converged ? "global route converged" : `${layout.globalRouting.totalOverflow} route overflow`}</span>
                <span>{layout.globalRouting.routes.length} routed nets · {layout.globalRouting.iterations.length} negotiation passes</span>
                <span>{layout.detailedRouting.converged ? "detailed route converged" : `${layout.detailedRouting.conflictCount} detail conflicts · ${layout.detailedRouting.blockedPinCount} blocked pins`}</span>
                <span>{layout.detailedRouting.totalWireLengthUm.toFixed(1)} µm detailed wire · {layout.detailedRouting.totalViaCount} vias · {layout.detailedRouting.iterations.length} repair passes</span>
                <span>{layout.detailedRouting.rejectedGeometryCount} illegal route shapes rejected before IR</span>
                <span>{layout.orphanRoutingShapesRemoved} disconnected metal/via shapes removed after routing</span>
                <span>{layout.detailedRouting.seededDeviceShapeCount} device shapes seeded before routing</span>
                <span>{layout.detailedRouting.trackRetryCount} indexed track retries · {layout.detailedRouting.layerEscalationCount} layer escalations</span>
                {layout.physicalBlocks.length > 0 && (
                  <span>{layout.physicalBlocks.filter((block) => block.verified).length}/{layout.physicalBlocks.length} frozen blocks locally DRC-clean · {layout.physicalBlocks.reduce((count, block) => count + block.interfacePins.length, 0)} interface pins</span>
                )}
                {layout.standardCellLibrary.generated && (
                  <span>{layout.standardCellLibrary.cells.length} packaged cells · {layout.standardCellLibrary.libraryName}</span>
                )}
                <span>{layout.timing.estimatedWorstDelayNs.toFixed(4)} ns worst path · {layout.timing.paths.length} input/output paths</span>
                <span>{layout.timing.candidates.length} routed candidates timing-scored</span>
                {candidateTiming && (
                  <span>
                    selected {candidateTiming.strategy}{candidateTiming.timingDriven ? " timing-driven" : ""} · {candidateTiming.estimatedWorstDelayNs.toFixed(4)} ns · {candidateTiming.areaUm2.toFixed(2)} µm²
                  </span>
                )}
                <span>{layout.timing.timingTargetNs === null
                  ? "timing unconstrained"
                  : `${layout.timing.worstSlackNs !== null && layout.timing.worstSlackNs >= 0 ? "+" : ""}${layout.timing.worstSlackNs?.toFixed(4)} ns worst slack · ${layout.timing.worstSlackNs !== null && layout.timing.worstSlackNs >= 0 ? "MET" : "VIOLATED"}`}</span>
                {layout.timing.criticalPath !== null && (() => {
                  const path = layout.timing.paths[layout.timing.criticalPath];
                  return <span>critical {path.inputPin} → {path.outputPin} · {path.netNames.join(" → ")}</span>;
                })()}
                <span>{layout.tapeout.fits ? "FITS" : "EXCEEDS"} {layout.tapeout.name} · {layout.tapeout.geometryWidthUm.toFixed(2)} × {layout.tapeout.geometryHeightUm.toFixed(2)} µm used</span>
                <span>{(layout.tapeout.areaUtilization * 100).toFixed(4)}% tapeout area · {layout.tapeout.shapesOutsideTapeout} shapes outside</span>
                {layout.tapeout.shapesOutsideFloorplan > 0 && <span>{layout.tapeout.shapesOutsideFloorplan} shapes extend beyond synthesized floorplan</span>}
              </>
            ) : null;
          })()}
        </div>
        <p className="physical-sidebar-section">Physical DRC</p>
        <div className={`physical-drc-status ${drc?.errorCount ? "has-errors" : "clean"}`}>
          <strong>{drc ? (drc.errorCount ? `${drc.errorCount} errors` : "DRC clean") : "Checking…"}</strong>
          <span>{drc ? `${drc.warningCount} warnings · ${drc.diagnostics.length} results` : "Rust rule deck"}</span>
        </div>
        {drc && <div className="physical-build-statistics">
          <strong>Build report</strong>
          {Object.entries(drc.byCategory).map(([name, count]) => <span key={name}>{name} <b>{count}</b></span>)}
          {Object.entries(drc.byOrigin).map(([name, count]) => <span key={name}>{name} <b>{count}</b></span>)}
          {buildReport && <small>{buildReport.elapsedMs} ms · overflow {buildReport.globalRoutingOverflow} · conflicts {buildReport.detailedRoutingConflicts} · rejected {buildReport.rejectedGeometryCount} · orphan cleanup {buildReport.orphanRoutingShapesRemoved}</small>}
        </div>}
        <div className="physical-drc-filters">
          <select aria-label="Filter DRC by layer" value={drcLayer} onChange={(event) => setDrcLayer(event.target.value)}>
            <option value="all">All layers</option>
            {drcLayers.map((layer) => <option key={layer} value={layer}>{layer}</option>)}
          </select>
          <select aria-label="Filter DRC by rule" value={drcRule} onChange={(event) => setDrcRule(event.target.value)}>
            <option value="all">All rules</option>
            {drcRules.map((rule) => <option key={rule} value={rule}>{rule}</option>)}
          </select>
          <select aria-label="Filter DRC by severity" value={drcSeverity} onChange={(event) => setDrcSeverity(event.target.value)}>
            <option value="all">All severities</option>
            <option value="error">Errors</option>
            <option value="warning">Warnings</option>
          </select>
          <select aria-label="Filter DRC by net" value={drcNet} onChange={(event) => setDrcNet(event.target.value)}>
            <option value="all">All nets</option>
            {drcNets.map((net) => <option key={net} value={net}>Net {net}: {layout?.nets.find((item) => item.id === net)?.name ?? "unnamed"}</option>)}
          </select>
        </div>
        <div className="physical-drc-list">
          {filteredDiagnostics.map(({ diagnostic, index }) => (
            <button
              key={`${diagnostic.ruleId}-${index}`}
              className={selectedDiagnostic === index ? "active" : ""}
              onClick={() => selectViolation(index, diagnostic)}
            >
              <span><strong>{diagnostic.ruleId}</strong><i>{diagnostic.layer}</i></span>
              <small>{diagnostic.message}</small>
              <em>{diagnostic.measured.toFixed(4)} µm measured · {diagnostic.required.toFixed(4)} µm required</em>
            </button>
          ))}
          {drc && !filteredDiagnostics.length && <p>{drc.diagnostics.length ? "No results match these filters." : "No physical violations."}</p>}
        </div>
        <button className="physical-reset" disabled={!drc} onClick={onSaveDrc}>Save DRC report…</button>
        <button className="physical-reset" disabled={!layout} onClick={onSaveLayout}>Save .chippy_gds…</button>
        <button className="physical-reset" disabled={!layout} onClick={onExportGds}>Export binary GDSII…</button>
        <button className="physical-reset" disabled={!layout} onClick={onExportLef}>Export LEF macro…</button>
        <p className="physical-sidebar-section">Layers</p>
        <button className="physical-reset" onClick={showAllLayers}>Show all layers</button>
        <button className="physical-reset" onClick={() => setShowDummyFill((current) => !current)}>
          {showDummyFill ? "Hide" : "Show"} dummy fill
        </button>
        <button className="physical-reset" onClick={() => {
          setSoloDummyFill((current) => !current);
          setShowDummyFill(true);
        }}>
          {soloDummyFill ? "Show electrical geometry" : "Solo dummy fill"}
        </button>
        <div className="physical-layers">
          {Object.entries(layers).map(([name, layer]) => (
            <label key={name}>
              <input
                type="checkbox"
                checked={visibleLayers.has(name)}
                onChange={() => toggleLayer(name)}
              />
              <i style={{ background: layer.color }} />
              <span>{layer.label}</span>
              <button type="button" onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                soloLayer(name);
              }}>Solo</button>
            </label>
          ))}
        </div>
        <p className="physical-help">Drag to pan · Right-drag to rotate · Wheel or pinch to zoom · Use the lower-right controls for exact views</p>
      </aside>
      <div className="physical-canvas" ref={host}>
        <div className="physical-camera-controls" role="group" aria-label="Physical view camera controls">
          <button type="button" onClick={() => orientView.current("top")}>Top</button>
          <button type="button" onClick={() => orientView.current("iso")}>Iso</button>
          <button type="button" onClick={() => orientView.current("front")}>Front</button>
          <button type="button" onClick={() => orientView.current("right")}>Right</button>
          <button type="button" className="fit" onClick={() => fitView.current()}>Fit</button>
        </div>
        {!layout && !buildError && <div className="physical-build-overlay">
          <div className="physical-build-card">
            <strong>Building Your Design…</strong>
            <span>{buildStages.find(({ id }) => id === buildProgress?.stage)?.label ?? "Starting physical generation"}</span>
            <div className="physical-build-timing">
              <span>Elapsed <b>{formatDuration(elapsedMs)}</b></span>
              <span>Estimate <b>{estimatedRemainingMs === null ? "Calculating…" : `~${formatDuration(estimatedRemainingMs)} remaining`}</b></span>
            </div>
            <div className="physical-build-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={buildPercent}>
              <i style={{ width: `${buildPercent}%` }} />
            </div>
            <small>{buildPercent}% complete · estimates adjust as routing complexity becomes known</small>
            <ol className="physical-build-checklist">
              {buildStages.map((stage) => {
                const complete = buildPercent >= stage.doneAt;
                const active = !complete && buildProgress?.stage === stage.id;
                return <li key={stage.id} className={complete ? "complete" : active ? "active" : ""}>
                  <i aria-hidden="true">{complete ? "✓" : active ? "●" : "○"}</i>
                  <span>{stage.label}</span>
                </li>;
              })}
            </ol>
            <small>Routing is correctness-first; difficult paths will continue searching.</small>
          </div>
        </div>}
        {!layout && buildError && <div className="physical-build-overlay">
          <div className="physical-build-card failed">
            <strong>Build Failed</strong>
            <span>{buildError}</span>
            <button type="button" onClick={onRetry}>Retry build</button>
          </div>
        </div>}
      </div>
    </div>
  );
}
