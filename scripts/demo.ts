import { createDemo } from '../src/core/demo';
import path from 'node:path';
console.log(await createDemo(path.resolve('.patch-demo')));
