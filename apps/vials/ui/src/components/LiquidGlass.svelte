<script lang="ts">
  // The "Liquid" look. Windows can only blur what is behind a window (acrylic); it cannot bend it.
  // A vial sits on the desktop, below every other window, so what is behind it is the wallpaper:
  // this draws that wallpaper itself, at the right place, and runs a refraction shader over it
  // (lens-bent edges, a little colour fringing, specular rim light). Redraws only when something
  // changes (move, resize, wallpaper, tint), never in a loop.
  import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
  import { onDestroy, onMount } from "svelte";
  import { api } from "../api";

  let { tint = 0.2, radius = 14, failed = $bindable(false) }: { tint?: number; radius?: number; failed?: boolean } =
    $props();

  let canvas: HTMLCanvasElement;
  let gl: WebGL2RenderingContext | null = null;
  let program: WebGLProgram | null = null;
  let texture: WebGLTexture | null = null;
  let img = { w: 0, h: 0, style: 10, tile: false };
  let ready = false;
  let raf = 0;
  const unlisten: Array<() => void> = [];

  const VERT = `#version 300 es
  in vec2 a;
  void main() { gl_Position = vec4(a, 0.0, 1.0); }`;

  const FRAG = `#version 300 es
  precision highp float;
  uniform sampler2D uTex;
  uniform vec2 uRes;      // canvas size, px
  uniform vec2 uWin;      // window top-left on its monitor, px
  uniform vec2 uOff;      // wallpaper top-left on the monitor, px
  uniform vec2 uDrawn;    // wallpaper size on the monitor, px
  uniform float uTile;    // 1 = repeat, 0 = clamp (black outside)
  uniform float uRadius;  // corner radius, px
  uniform float uBezel;   // width of the refracting rim, px
  uniform float uTint;    // 0..1
  uniform float uDpr;
  out vec4 outColor;

  float sdBox(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
  }

  vec3 wall(vec2 c, float lod) {
    vec2 uv = (uWin + c - uOff) / uDrawn;
    if (uTile > 0.5) uv = fract(uv);
    else if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) return vec3(0.0);
    return textureLod(uTex, uv, lod).rgb;
  }

  vec3 soft(vec2 c, float lod) {
    float o = 2.0 * uDpr;
    return 0.25 * (wall(c + vec2(o, 0.0), lod) + wall(c - vec2(o, 0.0), lod) +
                   wall(c + vec2(0.0, o), lod) + wall(c - vec2(0.0, o), lod));
  }

  void main() {
    vec2 c = vec2(gl_FragCoord.x, uRes.y - gl_FragCoord.y);
    vec2 half_ = uRes * 0.5;
    vec2 q = c - half_;
    float d = sdBox(q, half_, uRadius);
    float cover = clamp(0.5 - d, 0.0, 1.0);
    if (cover <= 0.0) { outColor = vec4(0.0); return; }

    float depth = -d;
    float e = 1.0;
    vec2 n = normalize(vec2(
      sdBox(q + vec2(e, 0.0), half_, uRadius) - sdBox(q - vec2(e, 0.0), half_, uRadius),
      sdBox(q + vec2(0.0, e), half_, uRadius) - sdBox(q - vec2(0.0, e), half_, uRadius)) + 1e-5);

    // Strongest bend right at the rim, easing to none at the inner edge of the bezel.
    float t = clamp(depth / uBezel, 0.0, 1.0);
    float bend = pow(1.0 - t, 2.6);
    float push = bend * uBezel * 0.95;
    float lod = mix(2.6, 0.4, bend);

    vec3 col;
    col.r = soft(c + n * push * 0.92, lod).r;
    col.g = soft(c + n * push * 1.00, lod).g;
    col.b = soft(c + n * push * 1.10, lod).b;

    // A little more colour, a slight lift, then the tint that keeps white labels readable.
    float lum = dot(col, vec3(0.299, 0.587, 0.114));
    col = mix(vec3(lum), col, 1.28) + 0.035;
    col = mix(col, vec3(0.05, 0.07, 0.10), uTint * 0.8);

    // Rim light from the top-left, a fainter one from the opposite corner, and a soft inner glow.
    vec2 L = normalize(vec2(-0.7, -1.0));
    float rim = smoothstep(2.2 * uDpr, 0.0, depth);
    float spec = pow(max(dot(n, L), 0.0), 2.5) + 0.45 * pow(max(dot(n, -L), 0.0), 2.5);
    col += rim * spec * 0.85;
    col += smoothstep(uBezel, 0.0, depth) * 0.05;
    col += (1.0 - smoothstep(0.0, 70.0 * uDpr, length(c))) * 0.05;

    outColor = vec4(col * cover, cover);
  }`;

  function compile(type: number, src: string): WebGLShader {
    const sh = gl!.createShader(type)!;
    gl!.shaderSource(sh, src);
    gl!.compileShader(sh);
    if (!gl!.getShaderParameter(sh, gl!.COMPILE_STATUS)) throw new Error(gl!.getShaderInfoLog(sh) ?? "shader");
    return sh;
  }

  function setup() {
    gl = canvas.getContext("webgl2", { premultipliedAlpha: true, alpha: true, antialias: false });
    if (!gl) throw new Error("no WebGL2");
    program = gl.createProgram()!;
    gl.attachShader(program, compile(gl.VERTEX_SHADER, VERT));
    gl.attachShader(program, compile(gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? "link");
    gl.useProgram(program);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(program, "a");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  }

  async function loadWallpaper() {
    const info = await api.getWallpaper();
    const el = new Image();
    el.src = info.dataUrl;
    await el.decode();
    // Mip-mapped copy capped at 2048 px: plenty for blurred glass, and cheap on GPU memory.
    const k = Math.min(1, 2048 / Math.max(el.naturalWidth, el.naturalHeight));
    const bmp = await createImageBitmap(el, {
      resizeWidth: Math.round(el.naturalWidth * k),
      resizeHeight: Math.round(el.naturalHeight * k),
      resizeQuality: "high",
    });
    const g = gl!;
    texture ??= g.createTexture();
    g.bindTexture(g.TEXTURE_2D, texture);
    g.texImage2D(g.TEXTURE_2D, 0, g.RGBA, g.RGBA, g.UNSIGNED_BYTE, bmp);
    g.generateMipmap(g.TEXTURE_2D);
    const wrap = info.tile ? g.REPEAT : g.CLAMP_TO_EDGE;
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_WRAP_S, wrap);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_WRAP_T, wrap);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_MIN_FILTER, g.LINEAR_MIPMAP_LINEAR);
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_MAG_FILTER, g.LINEAR);
    bmp.close();
    img = { w: el.naturalWidth, h: el.naturalHeight, style: info.style, tile: info.tile };
    ready = true;
  }

  /** Where Windows draws the wallpaper on a monitor of size (mw, mh): [offX, offY, drawnW, drawnH]. */
  function layout(mw: number, mh: number): [number, number, number, number] {
    const { w, h, style, tile } = img;
    if (tile) return [0, 0, w, h];
    if (style === 2) return [0, 0, mw, mh]; // stretch
    if (style === 0) return [(mw - w) / 2, (mh - h) / 2, w, h]; // center
    const s = style === 6 ? Math.min(mw / w, mh / h) : Math.max(mw / w, mh / h); // fit : fill / span
    return [(mw - w * s) / 2, (mh - h * s) / 2, w * s, h * s];
  }

  async function draw() {
    raf = 0;
    if (!gl || !program || !ready) return;
    const win = getCurrentWindow();
    const [pos, mon] = await Promise.all([win.outerPosition(), currentMonitor()]);
    if (!mon) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const [ox, oy, dw, dh] = layout(mon.size.width, mon.size.height);
    const u = (n: string) => gl!.getUniformLocation(program!, n);
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.uniform1i(u("uTex"), 0);
    gl.uniform2f(u("uRes"), w, h);
    gl.uniform2f(u("uWin"), pos.x - mon.position.x, pos.y - mon.position.y);
    gl.uniform2f(u("uOff"), ox, oy);
    gl.uniform2f(u("uDrawn"), dw, dh);
    gl.uniform1f(u("uTile"), img.tile ? 1 : 0);
    gl.uniform1f(u("uRadius"), radius * dpr);
    gl.uniform1f(u("uBezel"), 26 * dpr);
    gl.uniform1f(u("uTint"), tint);
    gl.uniform1f(u("uDpr"), dpr);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  function schedule() {
    if (!raf) raf = requestAnimationFrame(() => void draw());
  }

  $effect(() => {
    void tint;
    void radius;
    schedule();
  });

  onMount(() => {
    (async () => {
      try {
        setup();
        await loadWallpaper();
        schedule();
        const ro = new ResizeObserver(schedule);
        ro.observe(canvas);
        unlisten.push(() => ro.disconnect());
        const win = getCurrentWindow();
        unlisten.push(
          await win.onMoved(schedule),
          await win.onResized(schedule),
          await win.onScaleChanged(schedule),
          // A slideshow or a new wallpaper shows up the next time the vial is clicked.
          await win.onFocusChanged(async ({ payload }) => {
            if (payload) {
              await loadWallpaper().catch(() => {});
              schedule();
            }
          }),
        );
      } catch (e) {
        console.warn("liquid glass unavailable:", e);
        failed = true;
      }
    })();
  });

  onDestroy(() => {
    if (raf) cancelAnimationFrame(raf);
    unlisten.forEach((f) => f());
  });
</script>

<canvas bind:this={canvas} class="liquid" aria-hidden="true"></canvas>

<style>
  .liquid {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: -1;
    pointer-events: none;
  }
</style>
