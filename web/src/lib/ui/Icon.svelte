<script lang="ts" module>
  const PATHS = {
    dashboard: 'M4 4h7v9H4zM13 4h7v5h-7zM13 11h7v9h-7zM4 15h7v5H4z',
    usage: 'M5 20V11M12 20V4M19 20v-6M3 20h18',
    sessions: 'M4 5h16v14H4zM8 10l3 2.5L8 15M13 15h3',
    limits: 'M4.5 17a8 8 0 1 1 15 0M12 13l3.5-4M12 13.5v.01',
    insights: 'M9 18h6M10 21h4M12 3a6 6 0 0 0-4 10.4c.7.7 1 1.5 1 2.6h6c0-1.1.3-1.9 1-2.6A6 6 0 0 0 12 3z',
    alerts: 'M6 10a6 6 0 1 1 12 0c0 5 2 7 2 7H4s2-2 2-7M10 20.5a2.2 2.2 0 0 0 4 0',
    projects: 'M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5z',
    settings: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4',
    system: 'M12 3l8 3v6c0 4.8-3.4 8-8 9-4.6-1-8-4.2-8-9V6zM9 12l2.2 2.2L15.5 10',
    search: 'M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM20 20l-4.7-4.7',
    sun: 'M12 16.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9zM12 2.5v2M12 19.5v2M4.6 4.6 6 6M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4',
    moon: 'M20 14.6A8.2 8.2 0 0 1 9.4 4 8.2 8.2 0 1 0 20 14.6z',
    monitor: 'M3.5 5h17v11h-17zM9 20h6M12 16v4',
    globe: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3.5 9h17M3.5 15h17M12 3c2.4 2.6 3.6 5.6 3.6 9s-1.2 6.4-3.6 9c-2.4-2.6-3.6-5.6-3.6-9S9.6 5.6 12 3z',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    x: 'M6 6l12 12M18 6L6 18',
    chevron_down: 'M6 9l6 6 6-6',
    chevron_right: 'M9 6l6 6-6 6',
    chevron_left: 'M15 6l-6 6 6 6',
    warning: 'M12 4.5l8.5 15h-17zM12 10v4.5M12 17.2v.3',
    info: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5.5M12 7.8v.3',
    critical: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM9 9l6 6M15 9l-6 6',
    plug: 'M9 3v4.5M15 3v4.5M6.5 7.5h11V11a5.5 5.5 0 0 1-11 0zM12 16.5V21',
    download: 'M12 4v11M7 10.5l5 5 5-5M5 20h14',
    menu: 'M4 6.5h16M4 12h16M4 17.5h16',
    table: 'M4 5h16v14H4zM4 10h16M4 14.5h16M10 10v9',
    chart: 'M4 20h16M7 20v-6M12 20V8M17 20v-9',
    calendar: 'M4 6h16v14H4zM4 10.5h16M8.5 3.5v4M15.5 3.5v4',
    clock: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7.5V12l3 2',
    lock: 'M6 11h12v9.5H6zM8.5 11V8a3.5 3.5 0 0 1 7 0v3',
    bolt: 'M13 3 5.5 13.5h5.5L10 21l8-10.5h-5.5z',
    help: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM9.6 9.4a2.5 2.5 0 1 1 3.4 2.4c-.6.3-1 .8-1 1.5v.4M12 16.8v.3',
    refresh: 'M19.5 12a7.5 7.5 0 1 1-2.2-5.3M19.5 4.5v4.2h-4.2',
    external: 'M14 4h6v6M20 4l-9 9M18 14v6H4V6h6',
    terminal: 'M4 5h16v14H4zM7.5 9.5l3 2.5-3 2.5M12.5 15h4',
    dot: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
    folder_open: 'M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5V10M3 8v9l3-7h15l-3 9H5',
    pencil: 'M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4',
    plus: 'M12 5v14M5 12h14',
  } as const;
  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    size?: number;
    strokeWidth?: number;
    class?: string;
  }
  let { name, size = 18, strokeWidth = 1.75, class: cls = '' }: Props = $props();
</script>

<svg
  class="icon {cls}"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={PATHS[name]} />
</svg>

<style>
  .icon {
    flex: none;
    display: inline-block;
    vertical-align: middle;
  }
</style>
