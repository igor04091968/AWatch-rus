#!/usr/bin/env node
/**
 * Auto-versioning script for i18n locale files
 * Bumps version based on changes and updates all catalogs consistently
 */

import { readFileSync, writeFileSync, readdirSync } from 'fs';
import { join } from 'path';

const I18N_DIR = join(new URL('.', import.meta.url).pathname, '..');

function parseVersion(versionStr) {
    const match = versionStr.match(/^(\d+)\.(\d+)\.(\d+)$/);
    if (!match) return null;
    return {
        major: parseInt(match[1]),
        minor: parseInt(match[2]),
        patch: parseInt(match[3])
    };
}

function formatVersion(version) {
    return `${version.major}.${version.minor}.${version.patch}`;
}

function bumpVersion(version, type) {
    const v = parseVersion(version);
    if (!v) return version;
    
    switch (type) {
        case 'major':
            return formatVersion({ major: v.major + 1, minor: 0, patch: 0 });
        case 'minor':
            return formatVersion({ major: v.major, minor: v.minor + 1, patch: 0 });
        case 'patch':
        default:
            return formatVersion({ major: v.major, minor: v.minor, patch: v.patch + 1 });
    }
}

function getChangeType() {
    const args = process.argv.slice(2);
    if (args.includes('--major')) return 'major';
    if (args.includes('--minor')) return 'minor';
    return 'patch';
}

function updateLocaleFile(filePath, newVersion) {
    const content = readFileSync(filePath, 'utf-8');
    let catalog;
    
    try {
        catalog = JSON.parse(content);
    } catch (error) {
        console.error(`Error parsing ${filePath}: ${error.message}`);
        return false;
    }
    
    const oldVersion = catalog.version;
    catalog.version = newVersion;
    
    // Preserve formatting with 2-space indentation
    const updatedContent = JSON.stringify(catalog, null, 2);
    
    try {
        writeFileSync(filePath, updatedContent, 'utf-8');
        console.log(`✓ ${filePath}: ${oldVersion} → ${newVersion}`);
        return true;
    } catch (error) {
        console.error(`Error writing ${filePath}: ${error.message}`);
        return false;
    }
}

function main() {
    const changeType = getChangeType();
    
    console.log(`🔧 Auto-versioning i18n catalogs (${changeType} bump)\n`);
    
    // Get reference version from en-US.json
    const referencePath = join(I18N_DIR, 'en-US.json');
    let referenceVersion;
    
    try {
        const reference = JSON.parse(readFileSync(referencePath, 'utf-8'));
        referenceVersion = reference.version;
    } catch (error) {
        console.error(`Cannot read reference locale: ${error.message}`);
        process.exit(1);
    }
    
    if (!referenceVersion) {
        console.error('Reference locale has no version field');
        process.exit(1);
    }
    
    const newVersion = bumpVersion(referenceVersion, changeType);
    console.log(`Current version: ${referenceVersion}`);
    console.log(`New version: ${newVersion}\n`);
    
    // Update all locale files
    const files = readdirSync(I18N_DIR)
        .filter(f => f.endsWith('.json'));
    
    let successCount = 0;
    let failCount = 0;
    
    for (const file of files) {
        const filePath = join(I18N_DIR, file);
        if (updateLocaleFile(filePath, newVersion)) {
            successCount++;
        } else {
            failCount++;
        }
    }
    
    console.log(`\n─────────────────────────────────────`);
    console.log(`Updated: ${successCount} files`);
    console.log(`Failed: ${failCount} files`);
    
    if (failCount > 0) {
        process.exit(1);
    }
    
    console.log('\n✅ Version bump completed successfully!');
}

main();
