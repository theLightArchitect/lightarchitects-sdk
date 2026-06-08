import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
export const load: PageLoad = ({ params }) => {
  const suffix = params.project ? `/${params.project}` : '';
  throw redirect(301, `/diagrams${suffix}`);
};
