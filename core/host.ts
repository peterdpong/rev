import { inspect, filePatch, reviewPatches } from './git';
import type { FilePatch, Review, ReviewPatch, ReviewRequest } from './git';

export type ReviewResponse = {
  review: Review;
  patches: ReviewPatch[];
};

export class HostService {
  private closed = false;

  async call(method: string, params: any): Promise<ReviewResponse | FilePatch> {
    if (this.closed) {
      throw new Error('The local service is closing.');
    }

    if (!params || typeof params.repo !== 'string' || !params.repo.trim()) {
      throw new Error('Pass a repository folder.');
    }

    if (!['review', 'patch'].includes(method)) {
      throw new Error('Unknown action.');
    }

    const review = await inspect(params as ReviewRequest);
    if (method === 'patch') {
      return filePatch(review, params.path);
    }

    return { review, patches: await reviewPatches(review) };
  }

  close() {
    this.closed = true;
  }
}
