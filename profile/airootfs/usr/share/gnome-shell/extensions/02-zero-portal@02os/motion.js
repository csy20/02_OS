import Cairo from 'cairo';

// This is the approved scripts/boot-animation/02-logo.svg geometry. The first
// frame matches Plymouth's retained logo, including its short-display fit.
export const DURATION_SECONDS = 2.6;
const WHITE = [249 / 255, 249 / 255, 247 / 255];
const CYAN = [0, 186 / 255, 243 / 255];
const TWO = [
    ['M', 11, -133],
    ['C', 11, -207, 76, -266, 155, -266],
    ['C', 236, -266, 302, -208, 302, -131],
    ['C', 303, -90, 288, -54, 259, -23],
    ['L', 27, 224],
    // The SVG's quadratic curve, converted exactly to a cubic curve.
    ['C', 9, 243 + 1 / 3, -23 / 3, 254 + 1 / 3, -23, 257],
];

function smooth(value) {
    const t = Math.max(0, Math.min(1, value));
    return t * t * (3 - 2 * t);
}

function mix(from, to, amount) {
    return from + (to - from) * amount;
}

// The capsule is the original 0. Its two vertical sides shorten to zero while
// the four rounded corners morph continuously into a circular ring.
function zeroPath(cr, morph) {
    const k = 132 * 4 * (Math.SQRT2 - 1) / 3;
    cr.moveTo(0, mix(267, 132.5, morph));
    cr.curveTo(mix(-73, -k, morph), mix(267, 132.5, morph),
        -132, mix(208, k + 0.5, morph), -132, mix(134, 0.5, morph));
    cr.lineTo(-132, mix(-133, 0.5, morph));
    cr.curveTo(-132, mix(-206, 0.5 - k, morph),
        mix(-73, -k, morph), mix(-266, -131.5, morph), 0, mix(-266, -131.5, morph));
    cr.curveTo(mix(73, k, morph), mix(-266, -131.5, morph),
        132, mix(-206, 0.5 - k, morph), 132, mix(-133, 0.5, morph));
    cr.lineTo(132, mix(134, 0.5, morph));
    cr.curveTo(132, mix(207, k + 0.5, morph),
        mix(73, k, morph), mix(267, 132.5, morph), 0, mix(267, 132.5, morph));
    cr.closePath();
}

function drawTwo(cr, slide) {
    cr.save();
    // Only the area to the right of the 0 is visible. The 2 moves behind that
    // silhouette rather than fading or reappearing from its left side.
    cr.moveTo(-170, 267);
    cr.curveTo(-97, 267, -38, 207, -38, 134);
    cr.lineTo(-38, -133);
    cr.curveTo(-38, -206, -97, -266, -170, -266);
    cr.lineTo(-170, -2000);
    cr.lineTo(10000, -2000);
    cr.lineTo(10000, 2000);
    cr.lineTo(-170, 2000);
    cr.closePath();
    cr.clip();
    cr.translate(-500 * slide, 0);
    cr.newPath();
    for (const [command, ...values] of TWO) {
        if (command === 'M')
            cr.moveTo(...values);
        else if (command === 'C')
            cr.curveTo(...values);
        else
            cr.lineTo(...values);
    }
    cr.stroke();
    cr.moveTo(-101, 256);
    cr.lineTo(300, 256);
    cr.stroke();
    cr.restore();
}

export function drawPortalFrame(cr, width, height, elapsed) {
    // St.DrawingArea reuses its backing texture: clear it on every repaint so
    // an expanding hole does not retain white pixels from the previous frame.
    cr.setOperator(Cairo.Operator.CLEAR);
    cr.paint();
    cr.setOperator(Cairo.Operator.OVER);
    if (elapsed >= DURATION_SECONDS)
        return;

    const canvasWidth = Math.max(1, Math.floor(Math.min(width * 0.4,
        height * 0.66 * 512 / 448)));
    const scale = canvasWidth * 0.59375 / 680;
    // The 0 stays at its place in the logo and keeps its tall oval.
    const originX = -170;
    const originY = 0.5;
    const x = originX * scale + width / 2;
    const y = height / 2 + originY * scale;
    const halfWidth = 132;
    const expand = elapsed < 0.95 ? 0 : smooth((elapsed - 0.95) / 1.65);
    let cover = 1;
    for (const px of [0, width]) {
        for (const py of [0, height]) {
            const dx = (px - x) / scale;
            const dy = (py - y) / scale;
            const clamped = Math.max(-133.5, Math.min(133.5, dy));
            cover = Math.max(cover, Math.hypot(dx, dy - clamped) / halfWidth);
        }
    }
    const grow = mix(1, cover + 32 / halfWidth, expand);
    const open = elapsed < 1.45 ? 0 : smooth((elapsed - 1.45) / 0.15);
    const innerGrow = Math.max(0, (grow - 16 / halfWidth) * open);

    cr.setSourceRGB(...WHITE);
    cr.rectangle(0, 0, width, height);
    if (innerGrow > 0) {
        cr.newSubPath();
        cr.save();
        cr.translate(x, y);
        cr.scale(scale * innerGrow, scale * innerGrow);
        cr.translate(0, -originY);
        zeroPath(cr, 0);
        cr.restore();
        cr.setFillRule(Cairo.FillRule.EVEN_ODD);
    }
    cr.fill();
    cr.setFillRule(Cairo.FillRule.WINDING);

    cr.save();
    cr.translate(width / 2, height / 2);
    cr.scale(scale, scale);
    cr.setLineWidth(32);
    cr.setLineCap(Cairo.LineCap.ROUND);
    cr.setLineJoin(Cairo.LineJoin.ROUND);
    cr.setSourceRGB(...CYAN);
    if (elapsed < 0.95)
        drawTwo(cr, smooth((elapsed - 0.15) / 0.8));

    cr.translate(originX, 0);
    cr.translate(0, originY);
    cr.scale(grow, grow);
    cr.translate(0, -originY);
    cr.setLineWidth(32 / grow);
    if (expand > 0.88)
        cr.setSourceRGBA(...CYAN, 1 - smooth((expand - 0.88) / 0.12));
    cr.newPath();
    zeroPath(cr, 0);
    cr.stroke();
    cr.restore();
}
