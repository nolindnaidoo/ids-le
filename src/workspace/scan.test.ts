import { afterEach, describe, expect, it } from 'vitest';
import { _resetMockState, _setWorkspaceFiles, Uri } from '../__mocks__/vscode';
import {
	decodeText,
	listFiles,
	type ScanLimits,
	scanFiles,
	unreadNotes,
} from './scan';

const LIMITS: ScanLimits = {
	patterns: ['**/*'],
	excludes: ['**/node_modules/**'],
	maxFiles: 10,
	maxFileBytes: undefined,
};
const TOKEN = {
	isCancellationRequested: false,
	onCancellationRequested: () => ({ dispose: () => {} }),
};

afterEach(() => _resetMockState());

describe('listFiles', () => {
	it('lists in path order, leaves out the excluded, and can be rooted at a folder', async () => {
		_setWorkspaceFiles({
			'/w/b.txt': 'b',
			'/w/a.txt': 'a',
			'/w/node_modules/x.txt': 'x',
			'/w/sub/c.txt': 'c',
		});
		const all = await listFiles(undefined, LIMITS);
		expect(all.files.map((uri) => uri.path)).toEqual([
			'/w/a.txt',
			'/w/b.txt',
			'/w/sub/c.txt',
		]);
		expect(all.fileLimitReached).toBe(false);

		const sub = await listFiles(Uri.file('/w/sub') as never, LIMITS);
		expect(sub.files.map((uri) => uri.path)).toEqual(['/w/sub/c.txt']);
	});

	it('says when more files matched than the limit', async () => {
		_setWorkspaceFiles({ '/w/a': 'a', '/w/b': 'b', '/w/c': 'c' });
		const two = await listFiles(undefined, { ...LIMITS, maxFiles: 2 });
		expect(two.files).toHaveLength(2);
		expect(two.fileLimitReached).toBe(true);
		const three = await listFiles(undefined, { ...LIMITS, maxFiles: 3 });
		expect(three.fileLimitReached).toBe(false);
	});
});

describe('decodeText', () => {
	it('reads UTF-8 and refuses what is not text', () => {
		expect(decodeText(new TextEncoder().encode('héllo'))).toBe('héllo');
		expect(decodeText(new Uint8Array([0x61, 0x00, 0x62]))).toBeUndefined();
		expect(decodeText(new Uint8Array([0xff, 0xfe, 0x61]))).toBeUndefined();
	});
});

describe('scanFiles', () => {
	it('hands over each readable file and counts the ones it left', async () => {
		_setWorkspaceFiles({
			'/w/a.txt': 'small',
			'/w/big.txt': 'x'.repeat(50),
			'/w/bin': new Uint8Array([0, 1, 2]),
		});
		const { files } = await listFiles(undefined, LIMITS);
		const seen: string[] = [];
		const summary = await scanFiles(
			files,
			{ ...LIMITS, maxFileBytes: 10 },
			TOKEN as never,
			() => {},
			({ file, text }) => {
				seen.push(`${file}=${text}`);
				return undefined;
			},
		);
		expect(seen).toEqual(['/w/a.txt=small']);
		expect(summary).toEqual({
			read: 1,
			tooLarge: 1,
			notText: 1,
			stoppedEarly: false,
			cancelled: false,
		});
	});

	it('stops when the caller says so, and says it stopped only if files were left', async () => {
		_setWorkspaceFiles({ '/w/a': 'a', '/w/b': 'b', '/w/c': 'c' });
		const { files } = await listFiles(undefined, LIMITS);
		let calls = 0;
		const early = await scanFiles(
			files,
			LIMITS,
			TOKEN as never,
			() => {},
			() => {
				calls++;
				return false;
			},
		);
		expect(calls).toBe(1);
		expect(early.stoppedEarly).toBe(true);

		calls = 0;
		const atEnd = await scanFiles(
			files,
			LIMITS,
			TOKEN as never,
			() => {},
			() => {
				calls++;
				return calls < 3;
			},
		);
		expect(atEnd.stoppedEarly).toBe(false);
	});

	it('reports a cancel and reads no further', async () => {
		_setWorkspaceFiles({ '/w/a': 'a' });
		const { files } = await listFiles(undefined, LIMITS);
		const summary = await scanFiles(
			files,
			LIMITS,
			{ ...TOKEN, isCancellationRequested: true } as never,
			() => {},
			() => undefined,
		);
		expect(summary.cancelled).toBe(true);
		expect(summary.read).toBe(0);
	});
});

describe('unreadNotes', () => {
	const read = {
		read: 3,
		tooLarge: 0,
		notText: 0,
		fileLimitReached: false,
		stoppedEarly: false,
		cancelled: false,
	};

	it('is empty when everything was read', () => {
		expect(unreadNotes(read, LIMITS)).toEqual([]);
	});

	it('has a line for each thing left unread', () => {
		expect(
			unreadNotes(
				{
					...read,
					tooLarge: 2,
					notText: 1,
					fileLimitReached: true,
					stoppedEarly: true,
				},
				LIMITS,
			),
		).toEqual([
			'More files matched than the limit of 10. The rest were not read.',
			'The results limit was reached. The rest of the files were not read.',
			'2 file(s) larger than the safety limit were not read.',
			'1 file(s) that are not UTF-8 text were not read.',
		]);
	});
});
