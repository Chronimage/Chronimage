export type CullMode = 'compare' | 'grid' | 'swipe';

export type CullVerdict = 'keep_a' | 'keep_b' | 'reject_a' | 'reject_b' | 'reject_both' | 'accept_ai';

export interface CullPair {
  ids: [number, number];
  reason: string;
  similarity: number;
  keep: 0 | 1;
  issues_a: string[];
  issues_b: string[];
}
