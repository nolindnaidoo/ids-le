/**
 * The command line, parsed as the Rust CLI parses it: strictly, in order, and
 * refusing rather than defaulting.
 */
import { KIND_NAMES, type Kind, resolveFormat } from '../extract';
import { trim } from '../extract/text';
import { FLAGS } from './usage';

export interface WalkOptions {
	readonly hidden: boolean;
	readonly respectIgnore: boolean;
}

export interface Cli {
	readonly strict: boolean;
	readonly inputs: readonly string[];
	readonly stdin: boolean;
	/** The forced format, or null to infer one per file name. */
	readonly format: string | null;
	readonly kind: Kind | undefined;
	readonly walk: WalkOptions;
}

export function parse(args: readonly string[]): Cli {
	const inputs: string[] = [];
	let stdin = false;
	let strict = false;
	let format: string | null = null;
	let kind: Kind | undefined;
	let walk: WalkOptions = { hidden: false, respectIgnore: true };

	for (let index = 0; index < args.length; index++) {
		const arg = args[index] as string;
		if (arg.startsWith('-') && !FLAGS.includes(arg)) {
			throw new Error(`${arg} is not an option. Try --help.`);
		}
		if (arg === '--format') {
			index += 1;
			const value = args[index];
			if (value === undefined) throw new Error('--format needs a format');
			format = resolveFormat(value, undefined);
			continue;
		}
		if (arg === '--kind') {
			index += 1;
			kind = kindOf(args[index]);
			continue;
		}
		if (arg === '--stdin') stdin = true;
		if (arg === '--strict') strict = true;
		if (arg === '--hidden') walk = { ...walk, hidden: true };
		if (arg === '--no-ignore') walk = { ...walk, respectIgnore: false };
		if (!arg.startsWith('-')) inputs.push(arg);
	}

	if (stdin && inputs.length > 0) {
		throw new Error('reading from stdin takes no file arguments');
	}
	if (!stdin && inputs.length === 0) {
		throw new Error('name a file or a directory to read. Try --help.');
	}
	return Object.freeze({ strict, inputs, stdin, format, kind, walk });
}

/** The crate's `Kind::from_name`: Rust's trim, then lowercase. */
export function kindOf(value: string | undefined): Kind {
	const names = KIND_NAMES.join(', ');
	if (value === undefined) throw new Error(`--kind needs one of ${names}`);
	const name = trim(value).toLowerCase();
	const kind = KIND_NAMES.find((candidate) => candidate === name);
	if (kind === undefined) {
		throw new Error(`${value} is not a kind. Try one of ${names}.`);
	}
	return kind;
}
