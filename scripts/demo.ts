import { createDemo } from '../core/demo';
import path from 'node:path';
console.log(await createDemo(path.resolve('.rev-demo')));
