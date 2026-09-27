// Checks that the app's `@media (prefers-reduced-motion: reduce)` CSS
// actually reaches the elements that carry transform transitions (#1210).
//
// A reduced-motion rule can silently lose on specificity or layer order to
// the very utility it's meant to override, and a test that only toggles the
// OS setting can't tell "rule applied" from "rule lost". So instead: copy
// every rule inside a reduce media block into an unconditional <style>
// (keeping its @layer, so it cascades exactly as it would under the real
// preference), then read the computed transition-property of every element
// and pseudo-element in the rendered app.
import { executeScript, type DriverSession } from './webdriver-client';

interface ReducedMotionReport {
  /** Elements + pseudo-elements inspected. */
  checked: number;
  /** Of those, how many transitioned a transform/size property before injection. */
  movingBefore: number;
  /** Rules copied out of reduce media blocks. */
  injectedRules: number;
  /** Still transitioning a property outside the reduced-motion allow-list. */
  offenders: string[];
}

const PAGE_SCRIPT = String.raw`
  // Properties that move or resize something. 'all' counts: it includes transform.
  const MOVING = /(^|,\s*)(all|transform|translate|scale|rotate|width|height|max-width|max-height|inset|top|left|right|bottom|margin[\w-]*)\s*(,|$)/;
  const layerPath = [];
  const copied = [];
  function wrap(text) {
    return layerPath.reduceRight((inner, name) => '@layer ' + name + ' {' + inner + '}', text);
  }
  function collect(rules) {
    for (const rule of rules) {
      if (rule instanceof CSSMediaRule) {
        if (/prefers-reduced-motion:\s*reduce/.test(rule.conditionText)) {
          for (const inner of rule.cssRules) copied.push(wrap(inner.cssText));
        } else {
          collect(rule.cssRules);
        }
      } else if (typeof CSSLayerBlockRule !== 'undefined' && rule instanceof CSSLayerBlockRule) {
        layerPath.push(rule.name);
        collect(rule.cssRules);
        layerPath.pop();
      } else if (rule.cssRules) {
        collect(rule.cssRules);
      }
    }
  }
  for (const sheet of document.styleSheets) {
    try { collect(sheet.cssRules); } catch { /* cross-origin sheet */ }
  }

  function describe(el, pseudo) {
    const cls = typeof el.className === 'string' ? el.className.trim().split(/\s+/).slice(0, 4).join('.') : '';
    return el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (cls ? '.' + cls : '') + (pseudo || '');
  }
  function isMoving(cs) {
    const durations = cs.transitionDuration.split(',').map((d) => parseFloat(d));
    return durations.some((d) => d > 0) && MOVING.test(cs.transitionProperty);
  }

  const targets = [];
  for (const el of document.body.querySelectorAll('*')) {
    for (const pseudo of [null, '::before', '::after']) targets.push([el, pseudo]);
  }
  const movingBefore = targets.filter(([el, p]) => isMoving(getComputedStyle(el, p))).length;

  const style = document.createElement('style');
  style.textContent = copied.join('\n');
  document.head.appendChild(style);
  try {
    const offenders = [];
    for (const [el, pseudo] of targets) {
      const cs = getComputedStyle(el, pseudo);
      if (isMoving(cs)) offenders.push(describe(el, pseudo) + ' → ' + cs.transitionProperty);
    }
    return { checked: targets.length, movingBefore, injectedRules: copied.length, offenders };
  } finally {
    style.remove();
  }
`;

export async function checkReducedMotion(session: DriverSession): Promise<void> {
  const report = await executeScript<ReducedMotionReport>(session, PAGE_SCRIPT);
  console.log(
    `Reduced motion: ${report.injectedRules} reduce rules injected; ${report.checked} elements/pseudo-elements checked, ` +
      `${report.movingBefore} transitioned a transform/size property before, ${report.offenders.length} still do after.`,
  );
  // Guard against a vacuous pass: if nothing moved to begin with, this run
  // proved nothing about the reduce rules.
  if (report.movingBefore === 0) {
    throw new Error('Reduced-motion check is vacuous: no element transitions a transform/size property in this view.');
  }
  if (report.offenders.length > 0) {
    throw new Error(
      `${report.offenders.length} element(s) still transition a transform/size property under prefers-reduced-motion:\n  ` +
        report.offenders.slice(0, 30).join('\n  '),
    );
  }
}
