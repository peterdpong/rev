import { createSampleRepo } from '../core/git';
import path from 'node:path';
console.log(await createSampleRepo(path.resolve('.rev-sample')));
