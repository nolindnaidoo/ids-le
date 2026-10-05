import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { _resetMockState, _setConfig } from '../__mocks__/vscode';
import { KIND_NAMES } from '../extract';
import {
	CONFIG_DEFAULTS,
	isValidNotificationLevel,
	readConfig,
} from './config';

describe('config defaults parity with package.json', () => {
	const manifest = JSON.parse(
		readFileSync(join(__dirname, '..', '..', 'package.json'), 'utf8'),
	) as {
		contributes: {
			configuration: {
				properties: Record<string, { default: unknown; enum?: string[] }>;
			};
		};
	};
	const props = manifest.contributes.configuration.properties;
	const KEY_MAP: Record<string, keyof typeof CONFIG_DEFAULTS> = {
		'ids-le.clipboardIncludesPositions': 'clipboardIncludesPositions',
		'ids-le.copyToClipboardEnabled': 'copyToClipboardEnabled',
		'ids-le.kind': 'kind',
		'ids-le.notificationsLevel': 'notificationsLevel',
		'ids-le.openResultsSideBySide': 'openResultsSideBySide',
		'ids-le.safety.enabled': 'safetyEnabled',
		'ids-le.safety.fileSizeWarnBytes': 'safetyFileSizeWarnBytes',
		'ids-le.showPositions': 'showPositions',
		'ids-le.statusBar.enabled': 'statusBarEnabled',
		'ids-le.telemetryEnabled': 'telemetryEnabled',
	};

	it('covers every declared setting', () => {
		expect(Object.keys(props).sort()).toEqual(Object.keys(KEY_MAP).sort());
	});

	for (const [manifestKey, defaultsKey] of Object.entries(KEY_MAP)) {
		it(`${manifestKey} default matches`, () => {
			expect(CONFIG_DEFAULTS[defaultsKey]).toEqual(props[manifestKey]?.default);
		});
	}

	it('offers exactly the kinds the engine names, after all', () => {
		expect(props['ids-le.kind']?.enum).toEqual(['all', ...KIND_NAMES]);
	});
});

describe('readConfig', () => {
	afterEach(() => _resetMockState());

	it('falls back to all for a kind the engine does not know, so nothing is hidden by a typo', () => {
		_setConfig('ids-le.kind', 'guid');
		expect(readConfig().kind).toBe('all');
		_setConfig('ids-le.kind', 'ulid');
		expect(readConfig().kind).toBe('ulid');
	});

	it('falls back to the default for a value of the wrong type, and floors the size', () => {
		_setConfig('ids-le.openResultsSideBySide', 'yes');
		_setConfig('ids-le.safety.fileSizeWarnBytes', 5);
		expect(readConfig().openResultsSideBySide).toBe(true);
		expect(readConfig().safetyFileSizeWarnBytes).toBe(1000);
	});
});

describe('isValidNotificationLevel', () => {
	it('accepts the three declared levels and nothing else', () => {
		for (const level of ['all', 'important', 'silent'])
			expect(isValidNotificationLevel(level)).toBe(true);
		expect(isValidNotificationLevel('verbose')).toBe(false);
	});
});
