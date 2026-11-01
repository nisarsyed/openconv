// Render a Design-canvas board to plain HTML: run its renderVals with given props, expand sc-for and {{…}}.
// Usage: node render-board.mjs <board.dc.html> '{"appearance":"dark"}' <out.html>  (out must sit beside oc.css)
import fs from 'node:fs';
const [file, propsJson, out] = process.argv.slice(2);
const src = fs.readFileSync(file, 'utf8');
const m = src.match(/data-props=(['"])([\s\S]*?)\1>([\s\S]*?)<\/script>/);
const unesc = s => s.replace(/&quot;/g, '"').replace(/&amp;/g, '&');
const spec = JSON.parse(unesc(m[2]));
const props = {};
for (const [k, v] of Object.entries(spec)) if (!k.startsWith('$') && v && 'default' in v) props[k] = v.default;
Object.assign(props, JSON.parse(propsJson));
class DCLogic { constructor(p) { this.props = p; } }
const Component = new Function('DCLogic', m[3] + '\nreturn Component;')(DCLogic);
const vals = new Component(props).renderVals();
const helmet = (src.match(/<helmet>([\s\S]*?)<\/helmet>/) || [, ''])[1];
let body = src.slice(src.indexOf('</helmet>') + 9, src.indexOf('</x-dc>'));
const get = (obj, path) => path.split('.').reduce((o, k) => (o == null ? o : o[k]), obj);
body = body.replace(/<sc-for list="\{\{(\w+)\}\}" as="(\w+)"[^>]*>([\s\S]*?)<\/sc-for>/g, (_, list, as, inner) =>
  (vals[list] || []).map(item => inner.replace(/\{\{([\w.]+)\}\}/g, (__, p) => p.startsWith(as + '.') ? String(get(item, p.slice(as.length + 1)) ?? '') : `{{${p}}}`)).join(''));
body = body.replace(/\{\{([\w.]+)\}\}/g, (_, p) => String(get(vals, p) ?? ''));
fs.writeFileSync(out, `<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="oc.css">${helmet}</head><body style="margin:0">${body}</body></html>`);
