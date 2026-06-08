import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
export const load: PageLoad = ({ params }) => {
  const suffix = params.path ? `/${params.path}` : '';
  throw redirect(301, `/builds${suffix}`);
};
