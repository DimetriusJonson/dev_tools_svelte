export type DiffKind = 'added' | 'removed' | 'changed';

export interface InlineRange {
	start: number;
	end: number;
}

export interface LineHighlight {
	line: number;
	kind: DiffKind;
	inline: InlineRange[];
}

export interface DiffStats {
	added: number;
	removed: number;
	changed: number;
}

export interface DiffResult {
	left: LineHighlight[];
	right: LineHighlight[];
	stats: DiffStats;
	identical: boolean;
}

type OpType = 'equal' | 'delete' | 'insert';

interface Op {
	type: OpType;
	a?: number;
	b?: number;
}

interface Token {
	text: string;
	start: number;
	end: number;
}

const MAX_LCS_CELLS = 4_000_000;
const MAX_TOKEN_CELLS = 250_000;

export function splitLines(text: string): string[] {
	return text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
}

export function lineOffsets(text: string): number[] {
	const offsets = [0];
	for (let i = 0; i < text.length; i += 1) {
		if (text[i] === '\n') offsets.push(i + 1);
	}
	return offsets;
}

function lcsOps(a: string[], b: string[]): Op[] {
	const n = a.length;
	const m = b.length;
	const ops: Op[] = [];

	if ((n + 1) * (m + 1) > MAX_LCS_CELLS) {
		let prefix = 0;
		while (prefix < n && prefix < m && a[prefix] === b[prefix]) {
			ops.push({ type: 'equal', a: prefix, b: prefix });
			prefix += 1;
		}
		let suffix = 0;
		while (
			suffix < n - prefix &&
			suffix < m - prefix &&
			a[n - 1 - suffix] === b[m - 1 - suffix]
		) {
			suffix += 1;
		}
		for (let i = prefix; i < n - suffix; i += 1) ops.push({ type: 'delete', a: i });
		for (let j = prefix; j < m - suffix; j += 1) ops.push({ type: 'insert', b: j });
		for (let k = suffix; k > 0; k -= 1) {
			ops.push({ type: 'equal', a: n - k, b: m - k });
		}
		return ops;
	}

	const width = m + 1;
	const dp = new Int32Array((n + 1) * (m + 1));
	for (let i = n - 1; i >= 0; i -= 1) {
		for (let j = m - 1; j >= 0; j -= 1) {
			dp[i * width + j] =
				a[i] === b[j]
					? dp[(i + 1) * width + j + 1] + 1
					: Math.max(dp[(i + 1) * width + j], dp[i * width + j + 1]);
		}
	}

	let i = 0;
	let j = 0;
	while (i < n && j < m) {
		if (a[i] === b[j]) {
			ops.push({ type: 'equal', a: i, b: j });
			i += 1;
			j += 1;
		} else if (dp[(i + 1) * width + j] >= dp[i * width + j + 1]) {
			ops.push({ type: 'delete', a: i });
			i += 1;
		} else {
			ops.push({ type: 'insert', b: j });
			j += 1;
		}
	}
	while (i < n) {
		ops.push({ type: 'delete', a: i });
		i += 1;
	}
	while (j < m) {
		ops.push({ type: 'insert', b: j });
		j += 1;
	}

	return ops;
}

function tokenizeLine(line: string): Token[] {
	const looksLikeXml = line.includes('<') || line.includes('>');
	const regex = looksLikeXml
		? /<[^>]*>|[^<]+/g
		: /"(?:[^"\\]|\\.)*"|\s+|[{}[\],:]+|[^\s{}[\],"]+/g;
	const tokens: Token[] = [];
	let match: RegExpExecArray | null;
	while ((match = regex.exec(line)) !== null) {
		tokens.push({ text: match[0], start: match.index, end: match.index + match[0].length });
	}
	return tokens;
}

function mergeRanges(ranges: InlineRange[]): InlineRange[] {
	if (ranges.length === 0) return ranges;
	const sorted = [...ranges].sort((a, b) => a.start - b.start);
	const merged: InlineRange[] = [sorted[0]];
	for (let i = 1; i < sorted.length; i += 1) {
		const current = sorted[i];
		const last = merged[merged.length - 1];
		if (current.start <= last.end) {
			last.end = Math.max(last.end, current.end);
		} else {
			merged.push({ ...current });
		}
	}
	return merged;
}

function tokenDiff(leftLine: string, rightLine: string): { left: InlineRange[]; right: InlineRange[] } {
	const leftTokens = tokenizeLine(leftLine);
	const rightTokens = tokenizeLine(rightLine);
	const n = leftTokens.length;
	const m = rightTokens.length;

	if (n === 0 || m === 0 || (n + 1) * (m + 1) > MAX_TOKEN_CELLS) {
		return {
			left: leftLine.length ? [{ start: 0, end: leftLine.length }] : [],
			right: rightLine.length ? [{ start: 0, end: rightLine.length }] : []
		};
	}

	const width = m + 1;
	const dp = new Int32Array((n + 1) * (m + 1));
	for (let i = n - 1; i >= 0; i -= 1) {
		for (let j = m - 1; j >= 0; j -= 1) {
			dp[i * width + j] =
				leftTokens[i].text === rightTokens[j].text
					? dp[(i + 1) * width + j + 1] + 1
					: Math.max(dp[(i + 1) * width + j], dp[i * width + j + 1]);
		}
	}

	const leftRanges: InlineRange[] = [];
	const rightRanges: InlineRange[] = [];
	let i = 0;
	let j = 0;
	while (i < n && j < m) {
		if (leftTokens[i].text === rightTokens[j].text) {
			i += 1;
			j += 1;
		} else if (dp[(i + 1) * width + j] >= dp[i * width + j + 1]) {
			leftRanges.push({ start: leftTokens[i].start, end: leftTokens[i].end });
			i += 1;
		} else {
			rightRanges.push({ start: rightTokens[j].start, end: rightTokens[j].end });
			j += 1;
		}
	}
	while (i < n) {
		leftRanges.push({ start: leftTokens[i].start, end: leftTokens[i].end });
		i += 1;
	}
	while (j < m) {
		rightRanges.push({ start: rightTokens[j].start, end: rightTokens[j].end });
		j += 1;
	}

	const left = mergeRanges(leftRanges);
	const right = mergeRanges(rightRanges);

	if (left.length === 0 && right.length === 0 && leftLine !== rightLine) {
		return {
			left: leftLine.length ? [{ start: 0, end: leftLine.length }] : [],
			right: rightLine.length ? [{ start: 0, end: rightLine.length }] : []
		};
	}

	return { left, right };
}

function wholeLine(line: string, column: number): InlineRange[] {
	return line.length ? [{ start: 0, end: line.length }] : [{ start: column, end: column }];
}

export function diffTexts(leftText: string, rightText: string): DiffResult {
	const leftLines = splitLines(leftText);
	const rightLines = splitLines(rightText);
	const ops = lcsOps(leftLines, rightLines);

	const left: LineHighlight[] = [];
	const right: LineHighlight[] = [];
	const stats: DiffStats = { added: 0, removed: 0, changed: 0 };

	let index = 0;
	while (index < ops.length) {
		if (ops[index].type === 'equal') {
			index += 1;
			continue;
		}

		const deletes: number[] = [];
		const inserts: number[] = [];
		while (index < ops.length && ops[index].type !== 'equal') {
			const op = ops[index];
			if (op.type === 'delete' && op.a !== undefined) deletes.push(op.a);
			if (op.type === 'insert' && op.b !== undefined) inserts.push(op.b);
			index += 1;
		}

		const pairs = Math.min(deletes.length, inserts.length);
		for (let k = 0; k < pairs; k += 1) {
			const leftIndex = deletes[k];
			const rightIndex = inserts[k];
			const inline = tokenDiff(leftLines[leftIndex] ?? '', rightLines[rightIndex] ?? '');
			left.push({ line: leftIndex + 1, kind: 'changed', inline: inline.left });
			right.push({ line: rightIndex + 1, kind: 'changed', inline: inline.right });
			stats.changed += 1;
		}

		for (let k = pairs; k < deletes.length; k += 1) {
			const leftIndex = deletes[k];
			const text = leftLines[leftIndex] ?? '';
			left.push({ line: leftIndex + 1, kind: 'removed', inline: wholeLine(text, 0) });
			stats.removed += 1;
		}

		for (let k = pairs; k < inserts.length; k += 1) {
			const rightIndex = inserts[k];
			const text = rightLines[rightIndex] ?? '';
			right.push({ line: rightIndex + 1, kind: 'added', inline: wholeLine(text, 0) });
			stats.added += 1;
		}
	}

	return {
		left,
		right,
		stats,
		identical: left.length === 0 && right.length === 0
	};
}
