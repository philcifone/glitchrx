<script lang="ts">
  // A Lucide icon struck through, for "off" states Lucide does not draw
  // (apple-off, venetian-mask-off). Lucide's own -off icons break the shape
  // where the slash crosses it; here an SVG mask does the same for any
  // icon: a black line in the mask, a little wider than the slash, erases
  // the icon under it, and the slash is drawn in the gap. Paths come from
  // OFF_PATHS in lib/icons.ts, copied from Lucide (ISC license).
  let {
    paths,
    size = 16,
    strokeWidth = 1.75,
  }: { paths: readonly string[]; size?: number; strokeWidth?: number } = $props();

  // Mask ids are page-wide, so each icon needs its own.
  const id = $props.id();
  const SLASH = 'm2 2 20 20';
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <defs>
    <mask id="gap-{id}" maskUnits="userSpaceOnUse" x="0" y="0" width="24" height="24">
      <rect width="24" height="24" fill="white" stroke="none" />
      <path d={SLASH} stroke="black" stroke-width={strokeWidth + 3} />
    </mask>
  </defs>
  <g mask="url(#gap-{id})">
    {#each paths as d (d)}<path {d} />{/each}
  </g>
  <path d={SLASH} />
</svg>
