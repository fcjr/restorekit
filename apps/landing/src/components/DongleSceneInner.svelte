<script lang="ts">
  import { T, useTask, useThrelte } from "@threlte/core";
  import {
    Box3,
    CatmullRomCurve3,
    Fog,
    Group,
    Mesh,
    MeshBasicMaterial,
    PointLight,
    SphereGeometry,
    TubeGeometry,
    Vector3,
  } from "three";
  import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
  import { MeshoptDecoder } from "three/examples/jsm/libs/meshopt_decoder.module.js";
  import boardUrl from "../assets/dongle-lite.glb?url";

  let {
    onports,
  }: {
    // Projected screen positions (fractions of the canvas) of the two USB-C
    // ports, for the HTML labels overlaid on the container.
    onports?: (host: { x: number; y: number }, target: { x: number; y: number }) => void;
  } = $props();

  const AMBER = "#e8a33d";
  const AMBER_BRIGHT = "#f0b255";

  // The actual board: dongle-lite-1s4l exported straight from the KiCad PCB
  // (kicad-cli pcb export glb --subst-models --include-tracks ... then
  // gltf-transform meshopt). Re-export if the layout changes, and refresh the
  // two port anchors below from the J1/J2 node translations in the raw GLB.
  // The real board is 77.1 x 22.1 mm; it's normalized to 1 unit = 10 mm,
  // lying flat, centered at the origin, host port on the left.
  const BOARD_L = 7.71;

  // J1 (host) and J2 (target) footprint origins in model space (meters).
  const J1 = new Vector3(0.107, 0.004, 0.104115);
  const J2 = new Vector3(0.107, 0.004, 0.170885);

  let board = $state<Group>();
  // Port positions in scene units, set once the board is placed.
  let hostPort = new Vector3(-3.4, 0.1, 0);
  let targetPort = new Vector3(3.4, 0.1, 0);
  let traceGeo = $state<TubeGeometry>();
  let curve = $state<CatmullRomCurve3>();

  const loader = new GLTFLoader();
  loader.setMeshoptDecoder(MeshoptDecoder);
  loader.load(boardUrl, (gltf) => {
    const holder = new Group();
    holder.add(gltf.scene);

    // Model space is Y-up with the board length along Z and the host port at
    // low Z, so a quarter turn puts the length on X with the host on the left.
    holder.rotateY(Math.PI / 2);
    holder.updateMatrixWorld(true);
    const box = new Box3().setFromObject(holder);
    const size = box.getSize(new Vector3());
    const k = BOARD_L / Math.max(size.x, size.z);
    holder.scale.setScalar(k);
    holder.updateMatrixWorld(true);
    const center = new Box3().setFromObject(holder).getCenter(new Vector3());
    holder.position.sub(center);
    holder.updateMatrixWorld(true);

    // Anchor the trace and the labels to where the connectors actually are.
    hostPort = J1.clone().applyMatrix4(holder.matrix);
    targetPort = J2.clone().applyMatrix4(holder.matrix);
    const lift = 0.1;
    const mid = (f: number, z: number) =>
      new Vector3()
        .lerpVectors(hostPort, targetPort, f)
        .setY(lift)
        .add(new Vector3(0, 0, z));
    curve = new CatmullRomCurve3(
      [
        hostPort.clone().setY(lift),
        mid(0.28, 0.08),
        mid(0.5, 0),
        mid(0.72, 0.12),
        targetPort.clone().setY(lift),
      ],
      false,
      "catmullrom",
      0.12,
    );
    traceGeo = new TubeGeometry(curve, 64, 0.014, 6);
    board = holder;
  });

  const traceMat = new MeshBasicMaterial({ color: AMBER, transparent: true, opacity: 0.35 });
  const pulseMat = new MeshBasicMaterial({ color: AMBER_BRIGHT, transparent: true, opacity: 0 });
  const pulseGeo = new SphereGeometry(0.06, 12, 12);

  const reduced =
    typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const { scene, camera, size } = useThrelte();
  scene.fog = new Fog("#050505", 8, 22);

  // Frame the whole board (plus connector overhang) regardless of container
  // aspect: back the camera off along its view direction until the board's
  // half-width fits the horizontal fov.
  const VIEW_DIR = new Vector3(0, 4.1, 7.6).normalize();
  const HALF_W = 4.9;
  const FOV = 30;
  let camDist = 0;
  function frame() {
    const aspect = size.current.width / Math.max(1, size.current.height);
    const halfV = Math.tan((FOV / 2) * (Math.PI / 180));
    const dist = Math.max(8.6, HALF_W / (halfV * aspect));
    if (Math.abs(dist - camDist) < 0.01) return;
    camDist = dist;
    const cam = camera.current;
    cam.position.copy(VIEW_DIR).multiplyScalar(dist);
    cam.lookAt(0, -0.25, 0);
  }

  let rig = $state<Group>();
  let pulseMesh = $state<Mesh>();
  let flashLight = $state<PointLight>();
  const pulsePos = new Vector3();
  const proj = new Vector3();

  // Pointer parallax on top of a gentle rocking sway.
  let tx = 0;
  let ty = 0;
  function onMove(e: MouseEvent) {
    tx = (e.clientX / innerWidth - 0.5) * 0.3;
    ty = (e.clientY / innerHeight - 0.5) * 0.12;
  }

  // One trigger loop: idle, a pulse runs host to target, an amber flash washes
  // over the target end on arrival (the mac just dropped into DFU), then it
  // settles and repeats. The board rocks instead of spinning so host stays
  // left and target right, matching the labels.
  const PERIOD = 5.5;
  const TRAVEL_START = 0.9;
  const TRAVEL_END = 2.3;
  const HOLD_END = 4.4;
  let t = 0;
  let sway = 0;
  const SPIN_BASE = -0.28;

  if (reduced) {
    traceMat.opacity = 0.45;
  }

  function toScreen(p: Vector3) {
    proj.copy(p);
    if (rig) rig.localToWorld(proj);
    proj.project(camera.current);
    return { x: (proj.x + 1) / 2, y: (1 - proj.y) / 2 };
  }

  useTask((delta) => {
    frame();
    if (rig) {
      if (!reduced) sway += delta;
      const target = SPIN_BASE + 0.3 * Math.sin(sway * 0.35);
      rig.rotation.y += (target + tx - rig.rotation.y) * 0.06;
      rig.rotation.x += (ty - rig.rotation.x) * 0.06;
      rig.updateMatrixWorld(true);
      if (board && onports) onports(toScreen(hostPort), toScreen(targetPort));
    }
    if (reduced) return;
    t = (t + delta) % PERIOD;

    if (pulseMesh && curve) {
      if (t >= TRAVEL_START && t < TRAVEL_END) {
        curve.getPointAt((t - TRAVEL_START) / (TRAVEL_END - TRAVEL_START), pulsePos);
        pulseMesh.position.copy(pulsePos);
        pulseMat.opacity = 1;
      } else {
        pulseMat.opacity = 0;
      }
    }

    let s = 0;
    if (t >= TRAVEL_END && t < TRAVEL_END + 0.35) {
      const k = (t - TRAVEL_END) / 0.35;
      s = k * (0.7 + 0.3 * Math.sin(k * 38)); // flicker as DFU latches
    } else if (t >= TRAVEL_END + 0.35 && t < HOLD_END) {
      s = 0.95;
    } else if (t >= HOLD_END && t < HOLD_END + 0.5) {
      s = 0.95 * (1 - (t - HOLD_END) / 0.5);
    }
    if (flashLight) flashLight.intensity = s * 2.5;
    traceMat.opacity = 0.35 + s * 0.25;
  });
</script>

<svelte:window onmousemove={onMove} />

<T.PerspectiveCamera
  makeDefault
  position={[0, 4.1, 7.6]}
  fov={30}
  oncreate={(c) => c.lookAt(0, -0.25, 0)}
/>

<T.GridHelper args={[26, 26, "#1c1b13", "#12110c"]} position.y={-1.3} />

<!-- warm key + cool fill + amber DFU flash over the target end -->
<T.AmbientLight intensity={0.55} />
<T.DirectionalLight position={[4, 7, 5]} intensity={2.2} color="#fff1da" />
<T.DirectionalLight position={[-5, 3, -4]} intensity={0.7} color="#9db4c4" />
<T.PointLight bind:ref={flashLight} position={[3.2, 1.1, 0]} intensity={0} color={AMBER} distance={5} />

<T.Group bind:ref={rig}>
  {#if board}
    <T is={board} />
  {/if}

  <!-- signal path host → target, and the DFU trigger pulse -->
  {#if traceGeo}
    <T.Mesh geometry={traceGeo} material={traceMat} />
  {/if}
  <T.Mesh bind:ref={pulseMesh} geometry={pulseGeo} material={pulseMat} />
</T.Group>
