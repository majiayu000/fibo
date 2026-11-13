import type { Card } from '../remote/hub';

/** The final title supplied by the host, including unbound terminals. */
export const cardTitle = (c: Card) => c.title || c.id;
