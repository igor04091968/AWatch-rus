#!/usr/bin/env node
/**
 * Locale validation script for i18n JSON files
 * Validates structure, required fields, and message format placeholders
 */

import { readFileSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const I18N_DIR = join(new URL('.', import.meta.url).pathname, '..');
const REQUIRED_FIELDS = ['version', 'language', 'fallback', 'messages'];
const MESSAGE_CATEGORIES = ['errors', 'info', 'warnings', 'prompts', 'status', 'choices'];

function validateLocaleFile(filePath) {
    const errors = [];
    const warnings = [];
    
    let content;
    try {
        content = readFileSync(filePath, 'utf-8');
    } catch (error) {
        return {
            valid: false,
            errors: [`Cannot read file: ${error.message}`],
            warnings: []
        };
    }
    
    let catalog;
    try {
        catalog = JSON.parse(content);
    } catch (error) {
        return {
            valid: false,
            errors: [`Invalid JSON: ${error.message}`],
            warnings: []
        };
    }
    
    // Check required top-level fields
    for (const field of REQUIRED_FIELDS) {
        if (!(field in catalog)) {
            errors.push(`Missing required field: "${field}"`);
        }
    }
    
    if (!catalog.version) {
        warnings.push('Version is empty or missing');
    } else if (!/^\d+\.\d+\.\d+$/.test(catalog.version)) {
        warnings.push(`Version "${catalog.version}" does not follow semver (X.Y.Z)`);
    }
    
    if (!catalog.language || typeof catalog.language !== 'string') {
        errors.push('Field "language" must be a non-empty string');
    }
    
    if (catalog.fallback !== null && typeof catalog.fallback !== 'string') {
        warnings.push('Field "fallback" should be null or a locale string');
    }
    
    // Validate messages structure
    if (!catalog.messages || typeof catalog.messages !== 'object') {
        errors.push('Field "messages" must be an object');
        return { valid: false, errors, warnings };
    }
    
    const messageKeys = Object.keys(catalog.messages);
    
    // Check for proper key naming convention (category.key)
    const invalidKeys = messageKeys.filter(key => !key.includes('.'));
    if (invalidKeys.length > 0) {
        warnings.push(`Keys without category prefix: ${invalidKeys.slice(0, 5).join(', ')}`);
    }
    
    // Check message categories coverage
    const foundCategories = new Set(messageKeys.map(k => k.split('.')[0]));
    const missingCategories = MESSAGE_CATEGORIES.filter(cat => !foundCategories.has(cat));
    if (missingCategories.length > 0) {
        warnings.push(`Missing message categories: ${missingCategories.join(', ')}`);
    }
    
    // Validate message format placeholders consistency
    const placeholderPattern = /\{(\d+)\}/g;
    const messagesWithPlaceholders = {};
    
    for (const [key, value] of Object.entries(catalog.messages)) {
        if (typeof value !== 'string') {
            errors.push(`Message "${key}" must be a string, got ${typeof value}`);
            continue;
        }
        
        if (value.trim() === '') {
            warnings.push(`Message "${key}" is empty`);
        }
        
        const matches = value.match(placeholderPattern);
        if (matches) {
            const indices = matches.map(m => parseInt(m.slice(1, -1)));
            const maxIndex = Math.max(...indices);
            const minIndex = Math.min(...indices);
            
            if (minIndex !== 0) {
                warnings.push(`Message "${key}": placeholder indices should start at 0`);
            }
            
            const expectedCount = maxIndex + 1;
            const uniqueIndices = new Set(indices);
            if (uniqueIndices.size !== expectedCount) {
                warnings.push(`Message "${key}": potentially missing placeholder indices (0-${maxIndex})`);
            }
            
            messagesWithPlaceholders[key] = {
                count: uniqueIndices.size,
                maxIndex
            };
        }
    }
    
    // Check for consistent placeholder usage across locales (if reference exists)
    const refLocalePath = join(I18N_DIR, 'en-US.json');
    if (basename(filePath) !== 'en-US.json' && refLocalePath !== filePath) {
        try {
            const refContent = JSON.parse(readFileSync(refLocalePath, 'utf-8'));
            const refMessages = refContent.messages || {};
            
            for (const [key, data] of Object.entries(messagesWithPlaceholders)) {
                if (refMessages[key]) {
                    const refMatches = refMessages[key].match(placeholderPattern);
                    if (refMatches) {
                        const refIndices = new Set(refMatches.map(m => parseInt(m.slice(1, -1))));
                        if (refIndices.size !== data.count) {
                            warnings.push(`Message "${key}": placeholder count differs from en-US (${data.count} vs ${refIndices.size})`);
                        }
                    }
                }
            }
        } catch (e) {
            // Reference locale may not exist
        }
    }
    
    return {
        valid: errors.length === 0,
        errors,
        warnings,
        stats: {
            totalMessages: messageKeys.length,
            messagesWithPlaceholders: Object.keys(messagesWithPlaceholders).length,
            categories: Array.from(foundCategories)
        }
    };
}

function main() {
    const args = process.argv.slice(2);
    const specificFile = args[0];
    
    let filesToValidate = [];
    
    if (specificFile) {
        filesToValidate = [join(I18N_DIR, specificFile)];
    } else {
        const files = readdirSync(I18N_DIR);
        filesToValidate = files
            .filter(f => f.endsWith('.json') && !f.startsWith('package'))
            .map(f => join(I18N_DIR, f));
    }
    
    if (filesToValidate.length === 0) {
        console.error('No locale files found to validate');
        process.exit(1);
    }
    
    let allValid = true;
    let totalErrors = 0;
    let totalWarnings = 0;
    
    console.log('🔍 Validating locale files...\n');
    
    for (const filePath of filesToValidate) {
        const fileName = basename(filePath);
        console.log(`📄 ${fileName}`);
        
        const result = validateLocaleFile(filePath);
        
        if (result.stats) {
            console.log(`   Messages: ${result.stats.totalMessages}`);
            console.log(`   Categories: ${result.stats.categories.join(', ')}`);
            console.log(`   With placeholders: ${result.stats.messagesWithPlaceholders}`);
        }
        
        if (result.errors.length > 0) {
            console.log(`   ❌ Errors (${result.errors.length}):`);
            result.errors.forEach(err => console.log(`      - ${err}`));
            totalErrors += result.errors.length;
            allValid = false;
        } else {
            console.log(`   ✅ No errors`);
        }
        
        if (result.warnings.length > 0) {
            console.log(`   ⚠️  Warnings (${result.warnings.length}):`);
            result.warnings.slice(0, 5).forEach(warn => console.log(`      - ${warn}`));
            if (result.warnings.length > 5) {
                console.log(`      ... and ${result.warnings.length - 5} more`);
            }
            totalWarnings += result.warnings.length;
        }
        
        console.log('');
    }
    
    console.log('─'.repeat(50));
    console.log(`Summary: ${totalErrors} errors, ${totalWarnings} warnings`);
    
    if (allValid) {
        console.log('✅ All locale files are valid!');
        process.exit(0);
    } else {
        console.log('❌ Validation failed. Please fix the errors above.');
        process.exit(1);
    }
}

main();
