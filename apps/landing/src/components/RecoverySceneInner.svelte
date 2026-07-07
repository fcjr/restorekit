<script lang="ts">
  import { T, useTask, useThrelte } from "@threlte/core";
  import {
    BoxGeometry,
    Color,
    EdgesGeometry,
    Fog,
    Group,
    LineBasicMaterial,
    Mesh,
    MeshBasicMaterial,
    PlaneGeometry,
    QuadraticBezierCurve3,
    SphereGeometry,
    TubeGeometry,
    Vector3,
  } from "three";

  const AMBER = "#e8a33d";
  const AMBER_BRIGHT = "#f0b255";
  const EDGE = "#8f8f7a";
  const EDGE_DEAD = "#454437";
  const CABLE = "#33322a";

  // Laptops are edge-outlined primitives; screens are emissive-looking planes.
  const baseEdges = new EdgesGeometry(new BoxGeometry(3, 0.14, 2.1));
  const lidEdges = new EdgesGeometry(new BoxGeometry(3, 1.95, 0.1));
  const screenGeo = new PlaneGeometry(2.72, 1.65);

  // USB-C cable sagging between the two machines, host (left) to target (right).
  const curve = new QuadraticBezierCurve3(
    new Vector3(-2.45, 0.15, 0.7),
    new Vector3(0, -1.05, 1.5),
    new Vector3(2.45, 0.15, 0.7),
  );
  const tubeGeo = new TubeGeometry(curve, 80, 0.022, 8);
  const pulseGeo = new SphereGeometry(0.09, 16, 16);

  const hostEdgeMat = new LineBasicMaterial({ color: EDGE });
  const targetEdgeMat = new LineBasicMaterial({ color: EDGE_DEAD });
  const cableMat = new MeshBasicMaterial({ color: CABLE });
  const hostScreenMat = new MeshBasicMaterial({ color: AMBER, transparent: true, opacity: 0.22 });
  const targetScreenMat = new MeshBasicMaterial({ color: AMBER, transparent: true, opacity: 0 });
  const pulseMat = new MeshBasicMaterial({ color: AMBER_BRIGHT, transparent: true, opacity: 0 });

  const edgeDead = new Color(EDGE_DEAD);
  const edgeLive = new Color(EDGE);

  const reduced =
    typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  const { scene } = useThrelte();
  scene.fog = new Fog("#050505", 9, 19);

  let rig = $state<Group>();
  let pulseMesh = $state<Mesh>();
  const pulsePos = new Vector3();

  // Pointer parallax: the whole bench leans a little toward the cursor.
  let tx = 0;
  let ty = 0;
  function onMove(e: MouseEvent) {
    tx = (e.clientX / innerWidth - 0.5) * 0.22;
    ty = (e.clientY / innerHeight - 0.5) * 0.1;
  }

  // One repair loop: idle, pulse travels the cable, dead screen flickers on,
  // holds, then powers back down and the loop restarts.
  const PERIOD = 6.5;
  const TRAVEL_START = 1.2;
  const TRAVEL_END = 3.0;
  const HOLD_END = 5.4;
  let t = 0;

  if (reduced) {
    targetScreenMat.opacity = 0.75;
    targetEdgeMat.color.copy(edgeLive);
  }

  useTask((delta) => {
    if (rig) {
      rig.rotation.y += (tx - rig.rotation.y) * 0.05;
      rig.rotation.x += (ty - rig.rotation.x) * 0.05;
    }
    if (reduced) return;
    t = (t + delta) % PERIOD;

    if (pulseMesh) {
      if (t >= TRAVEL_START && t < TRAVEL_END) {
        curve.getPointAt((t - TRAVEL_START) / (TRAVEL_END - TRAVEL_START), pulsePos);
        pulseMesh.position.copy(pulsePos);
        pulseMat.opacity = 1;
      } else {
        pulseMat.opacity = 0;
      }
    }

    let s = 0;
    if (t >= TRAVEL_END && t < TRAVEL_END + 0.5) {
      const k = (t - TRAVEL_END) / 0.5;
      s = k * (0.72 + 0.28 * Math.sin(k * 42)); // flicker while powering on
    } else if (t >= TRAVEL_END + 0.5 && t < HOLD_END) {
      s = 0.92;
    } else if (t >= HOLD_END && t < HOLD_END + 0.6) {
      s = 0.92 * (1 - (t - HOLD_END) / 0.6);
    }
    targetScreenMat.opacity = s * 0.8;
    targetEdgeMat.color.lerpColors(edgeDead, edgeLive, Math.min(1, s * 1.4));
  });
</script>

<svelte:window onmousemove={onMove} />

<T.PerspectiveCamera
  makeDefault
  position={[0, 2.6, 8.4]}
  fov={33}
  oncreate={(c) => c.lookAt(0, -0.3, 0)}
/>

<T.Group bind:ref={rig}>
  <T.GridHelper args={[28, 28, "#1c1b13", "#12110c"]} position.y={-1.18} />

  <!-- your machine -->
  <T.Group position.x={-3.9} rotation.y={0.55}>
    <T.LineSegments geometry={baseEdges} material={hostEdgeMat} />
    <T.Group position={[0, 0.07, -1.0]} rotation.x={-0.22}>
      <T.LineSegments geometry={lidEdges} material={hostEdgeMat} position={[0, 0.975, 0]} />
      <T.Mesh geometry={screenGeo} material={hostScreenMat} position={[0, 0.975, 0.06]} />
    </T.Group>
  </T.Group>

  <!-- the dead mac -->
  <T.Group position.x={3.9} rotation.y={-0.55}>
    <T.LineSegments geometry={baseEdges} material={targetEdgeMat} />
    <T.Group position={[0, 0.07, -1.0]} rotation.x={-0.22}>
      <T.LineSegments geometry={lidEdges} material={targetEdgeMat} position={[0, 0.975, 0]} />
      <T.Mesh geometry={screenGeo} material={targetScreenMat} position={[0, 0.975, 0.06]} />
    </T.Group>
  </T.Group>

  <T.Mesh geometry={tubeGeo} material={cableMat} />
  <T.Mesh bind:ref={pulseMesh} geometry={pulseGeo} material={pulseMat} />
</T.Group>
