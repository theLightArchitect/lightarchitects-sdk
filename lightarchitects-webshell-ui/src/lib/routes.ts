// routes.ts — SvelteKit migration complete (2026-06-08)
//
// matchRoute(), ScreenKey, RouteMatch, ROUTES, REDIRECTS, applyRedirects(),
// and the navigate() shim have all been removed:
//
//   • Routing:    SvelteKit file-system router in src/routes/
//   • Navigation: goto() from '$app/navigation'
//   • Redirects:  individual +page.ts files in src/routes/{ops,monitor,...}/
//   • Params:     page.params from '$app/state'
//
// BuildViewMode is preserved here as the canonical type for view tab keys
// used by BuildDetail.svelte and the /builds/[buildId]/[view] route.

export type BuildViewMode =
  | 'kanban'
  | 'list'
  | 'operator'
  | 'manifest'
  | 'plan'
  | 'comms'
  | 'pipeline'
  | 'autonomous'
  | 'decisions'
  | 'fleet';
