// web/esbuild.mjs
import * as esbuild from 'esbuild';

const isDev = process.argv.includes('--dev');

const buildOptions = {
  entryPoints: ['src/index.tsx'],
  bundle: true,
  minify: !isDev,
  sourcemap: isDev,
  outfile: 'dist/bundle.js',
  define: {
    'process.env.NODE_ENV': isDev ? '"development"' : '"production"',
  },
};

if (isDev) {
  let ctx = await esbuild.context(buildOptions);
  await ctx.watch();
  // 固定本地开发端口为 1420
  let { port } = await ctx.serve({ servedir: '.', port: 1420 });
  console.log(`[Dev] Server running at http://127.0.0.1:${port}`);
} else {
  await esbuild.build(buildOptions);
  console.log('[Build] Finished successfully.');
}