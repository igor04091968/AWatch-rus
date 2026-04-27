#!/usr/bin/env node
/**
 * Coverage analysis script for i18n locales
 * Compares all locales against the reference (en-US) and reports coverage
 */

import { readFileSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const I18N_DIR = join(new URL('.', import.meta.url).pathname, '..');
const REFERENCE_LOCALE = 'en-US.json';

function loadLocale(fileName) {
    const filePath = join(I18N_DIR, fileName);
    const content = readFileSync(filePath, 'utf-8');
    return JSON.parse(content);
}

function analyzeCoverage() {
    const files = readdirSync(I18N_DIR)
        .filter(f => f.endsWith('.json') && f !== REFERENCE_LOCALE && !f.startsWith('package'));
    
    if (files.length === 0) {
        console.error('No locale files found besides the reference');
        process.exit(1);
    }
    
    let reference;
    try {
        reference = loadLocale(REFERENCE_LOCALE);
    } catch (error) {
        console.error(`Cannot load reference locale (${REFERENCE_LOCALE}): ${error.message}`);
        process.exit(1);
    }
    
    const referenceKeys = Object.keys(reference.messages || {});
    const referenceCategories = categorizeKeys(referenceKeys);
    
    console.log('📊 i18n Coverage Analysis\n');
    console.log(`Reference: ${REFERENCE_LOCALE} (${referenceKeys.length} messages)\n`);
    
    const results = [];
    
    for (const fileName of files) {
        const localeName = basename(fileName, '.json');
        
        let catalog;
        try {
            catalog = loadLocale(fileName);
        } catch (error) {
            results.push({
                locale: localeName,
                error: error.message
            });
            continue;
        }
        
        const localeKeys = Object.keys(catalog.messages || {});
        const localeCategories = categorizeKeys(localeKeys);
        
        const missingKeys = referenceKeys.filter(k => !localeKeys.includes(k));
        const extraKeys = localeKeys.filter(k => !referenceKeys.includes(k));
        
        const coverage = referenceKeys.length > 0 
            ? ((referenceKeys.length - missingKeys.length) / referenceKeys.length * 100).toFixed(2)
            : 0;
        
        const categoryCoverage = {};
        for (const [category, refKeys] of Object.entries(referenceCategories)) {
            const localeCatKeys = localeCategories[category] || [];
            const missing = refKeys.filter(k => !localeCatKeys.includes(k));
            categoryCoverage[category] = {
                total: refKeys.length,
                translated: refKeys.length - missing.length,
                missing: missing.length,
                percent: refKeys.length > 0 
                    ? ((refKeys.length - missing.length) / refKeys.length * 100).toFixed(1)
                    : 100
            };
        }
        
        results.push({
            locale: localeName,
            version: catalog.version,
            language: catalog.language,
            fallback: catalog.fallback,
            totalMessages: localeKeys.length,
            referenceMessages: referenceKeys.length,
            missing: missingKeys,
            extra: extraKeys,
            coverage: parseFloat(coverage),
            categoryCoverage
        });
    }
    
    // Sort by coverage descending
    results.sort((a, b) => (b.coverage || 0) - (a.coverage || 0));
    
    // Print summary table
    console.log('┌' + '─'.repeat(78) + '┐');
    console.log('│ ' + padRight('Locale', 12) + ' │ ' + 
                padRight('Version', 10) + ' │ ' +
                padRight('Language', 10) + ' │ ' +
                padRight('Coverage', 10) + ' │ ' +
                padRight('Missing', 10) + ' │ ' +
                padRight('Extra', 10) + ' │');
    console.log('├' + '─'.repeat(78) + '┤');
    
    for (const result of results) {
        if (result.error) {
            console.log('│ ' + padRight(result.locale, 12) + ' │ ERROR: ' + result.error);
            continue;
        }
        
        const coverageStr = result.coverage.toFixed(2) + '%';
        const coverageColor = getCoverageIndicator(result.coverage);
        
        console.log('│ ' + padRight(result.locale, 12) + ' │ ' + 
                    padRight(result.version || 'N/A', 10) + ' │ ' +
                    padRight(result.language || 'N/A', 10) + ' │ ' +
                    padRight(coverageStr + coverageColor, 10) + ' │ ' +
                    padRight(result.missing.length.toString(), 10) + ' │ ' +
                    padRight(result.extra.length.toString(), 10) + ' │');
    }
    
    console.log('└' + '─'.repeat(78) + '┘\n');
    
    // Detailed breakdown per locale
    for (const result of results) {
        if (result.error || result.missing.length === 0) continue;
        
        console.log(`📋 ${result.locale} - Missing Keys (${result.missing.length}):`);
        
        // Group by category
        const missingByCategory = {};
        for (const key of result.missing) {
            const category = key.split('.')[0];
            if (!missingByCategory[category]) {
                missingByCategory[category] = [];
            }
            missingByCategory[category].push(key);
        }
        
        for (const [category, keys] of Object.entries(missingByCategory)) {
            console.log(`   ${category} (${keys.length}):`);
            keys.slice(0, 10).forEach(key => console.log(`      - ${key}`));
            if (keys.length > 10) {
                console.log(`      ... and ${keys.length - 10} more`);
            }
        }
        console.log('');
    }
    
    // Category coverage summary
    console.log('📈 Category Coverage Summary:\n');
    const categories = Object.keys(referenceCategories);
    
    console.log('┌' + '─'.repeat(68) + '┐');
    console.log('│ ' + padRight('Category', 20) + ' │ ' + 
                padRight('Ref Count', 12) + ' │ ' +
                'Average Coverage' + ' │');
    console.log('├' + '─'.repeat(68) + '┤');
    
    for (const category of categories) {
        const avgCoverage = results
            .filter(r => !r.error && r.categoryCoverage[category])
            .reduce((sum, r) => sum + parseFloat(r.categoryCoverage[category].percent), 0) / 
            Math.max(results.filter(r => !r.error).length, 1);
        
        console.log('│ ' + padRight(category, 20) + ' │ ' + 
                    padRight(referenceCategories[category].length.toString(), 12) + ' │ ' +
                    padRight(avgCoverage.toFixed(1) + '%', 16) + ' │');
    }
    
    console.log('└' + '─'.repeat(68) + '┘\n');
    
    // Recommendations
    console.log('💡 Recommendations:\n');
    
    const lowCoverageLocales = results.filter(r => !r.error && r.coverage < 80);
    if (lowCoverageLocales.length > 0) {
        console.log('1. Priority locales for translation:');
        lowCoverageLocales.forEach(r => {
            console.log(`   - ${r.locale}: ${r.missing.length} keys missing (${r.coverage.toFixed(1)}% coverage)`);
        });
        console.log('');
    }
    
    const allHaveFallback = results.every(r => !r.error && r.fallback);
    if (!allHaveFallback) {
        console.log('2. Consider adding fallback locale to all catalogs for better resilience');
        console.log('');
    }
    
    const versionMismatch = results.filter(r => !r.error && r.version !== reference.version);
    if (versionMismatch.length > 0) {
        console.log('3. Version mismatch detected. Consider bumping versions:');
        versionMismatch.forEach(r => {
            console.log(`   - ${r.locale}: ${r.version} (reference: ${reference.version})`);
        });
        console.log('');
    }
}

function categorizeKeys(keys) {
    const categories = {};
    for (const key of keys) {
        const category = key.split('.')[0];
        if (!categories[category]) {
            categories[category] = [];
        }
        categories[category].push(key);
    }
    return categories;
}

function padRight(str, length) {
    return (str || '').toString().padEnd(length, ' ');
}

function getCoverageIndicator(coverage) {
    if (coverage >= 95) return ' ✅';
    if (coverage >= 80) return ' ⚠️';
    return ' ❌';
}

analyzeCoverage();
