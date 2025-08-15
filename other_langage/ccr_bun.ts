#!/usr/bin/env bun

// Bun version of ccusage statusline command
// Usage: echo '{"session_id":"...", ...}' | bun run cc_statusline_bun.ts

import chalk from 'chalk';
import { z } from 'zod';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';

// Schema for hook input
const statuslineHookJsonSchema = z.object({
	session_id: z.string(),
	transcript_path: z.string(),
	cwd: z.string(),
	model: z.object({
		id: z.string(),
		display_name: z.string(),
	}),
	workspace: z.object({
		current_dir: z.string(),
		project_dir: z.string(),
	}),
	version: z.string().optional(),
});

// Types
type StatuslineHookJson = z.infer<typeof statuslineHookJsonSchema>;

interface UsageEntry {
	timestamp: string;
	model?: string;
	inputTokens?: number;
	outputTokens?: number;
	cacheCreationTokens?: number;
	cacheReadTokens?: number;
	costUSD?: number;
	// Fields from actual Claude Code JSONL format
	message?: {
		id?: string;
		model?: string;
		usage?: {
			input_tokens?: number;
			output_tokens?: number;
			cache_creation_input_tokens?: number;
			cache_read_input_tokens?: number;
		};
	};
	requestId?: string;
}

interface SessionBlock {
	startTime: Date;
	endTime: Date;
	isActive: boolean;
	costUSD: number;
	entries: UsageEntry[];
}

interface BurnRate {
	tokensPerMinute: number;
	tokensPerMinuteForIndicator: number;
	costPerHour: number;
}

interface ModelPricing {
	input_cost_per_token?: number;
	output_cost_per_token?: number;
	cache_creation_input_token_cost?: number;
	cache_read_input_token_cost?: number;
}

// Model pricing data (from LiteLLM)
const MODEL_PRICING: Record<string, ModelPricing> = {
	"claude-opus-4-1-20250805": {
		input_cost_per_token: 0.000015,
		output_cost_per_token: 0.000075,
		cache_creation_input_token_cost: 0.00001875,
		cache_read_input_token_cost: 0.0000015,
	},
	"claude-sonnet-4-20250514": {
		input_cost_per_token: 0.000003,
		output_cost_per_token: 0.000015,
		cache_creation_input_token_cost: 0.00000375,
		cache_read_input_token_cost: 0.0000003,
	},
	// Fallback for older models
	"claude-3-opus-20240229": {
		input_cost_per_token: 0.000015,
		output_cost_per_token: 0.000075,
		cache_creation_input_token_cost: 0.00001875,
		cache_read_input_token_cost: 0.0000015,
	},
	"claude-3.5-sonnet-20241022": {
		input_cost_per_token: 0.000003,
		output_cost_per_token: 0.000015,
		cache_creation_input_token_cost: 0.00000375,
		cache_read_input_token_cost: 0.0000003,
	},
};

// Get pricing for a model with fallback
function getModelPricing(modelName?: string): ModelPricing | null {
	if (!modelName) return null;
	
	// Direct match
	if (MODEL_PRICING[modelName]) {
		return MODEL_PRICING[modelName];
	}
	
	// Try partial matching
	for (const [key, pricing] of Object.entries(MODEL_PRICING)) {
		if (modelName.includes(key) || key.includes(modelName)) {
			return pricing;
		}
	}
	
	// Default to Opus pricing if no match
	if (modelName.toLowerCase().includes("opus")) {
		return MODEL_PRICING["claude-opus-4-1-20250805"];
	}
	if (modelName.toLowerCase().includes("sonnet")) {
		return MODEL_PRICING["claude-sonnet-4-20250514"];
	}
	
	return null;
}

// Calculate cost from tokens
function calculateCost(
	tokens: {
		input?: number;
		output?: number;
		cacheCreation?: number;
		cacheRead?: number;
	},
	pricing: ModelPricing
): number {
	let cost = 0;
	
	if (tokens.input && pricing.input_cost_per_token) {
		cost += tokens.input * pricing.input_cost_per_token;
	}
	if (tokens.output && pricing.output_cost_per_token) {
		cost += tokens.output * pricing.output_cost_per_token;
	}
	if (tokens.cacheCreation && pricing.cache_creation_input_token_cost) {
		cost += tokens.cacheCreation * pricing.cache_creation_input_token_cost;
	}
	if (tokens.cacheRead && pricing.cache_read_input_token_cost) {
		cost += tokens.cacheRead * pricing.cache_read_input_token_cost;
	}
	
	return cost;
}

// Utility functions
function formatCurrency(amount: number): string {
	return `$${amount.toFixed(2)}`;
}

function formatRemainingTime(remaining: number): string {
	const remainingHours = Math.floor(remaining / 60);
	const remainingMins = remaining % 60;
	if (remainingHours > 0) {
		return `${remainingHours}h ${remainingMins}m left`;
	}
	return `${remainingMins}m left`;
}

// Get Claude data directories
function getClaudePaths(): string[] {
	const paths: string[] = [];
	const home = os.homedir();
	
	// Check environment variable first
	const customPath = process.env.CLAUDE_CONFIG_DIR;
	if (customPath) {
		const customPaths = customPath.split(",").map(p => p.trim()).filter(p => p);
		paths.push(...customPaths);
	} else {
		// Default paths
		paths.push(path.join(home, ".config", "claude"));
		paths.push(path.join(home, ".claude"));
	}
	
	return paths.filter(p => {
		try {
			const stat = fs.statSync(p);
			return stat.isDirectory();
		} catch {
			return false;
		}
	});
}

// Load session usage data
async function loadSessionUsageById(sessionId: string): Promise<{ totalCost: number } | null> {
	const claudePaths = getClaudePaths();
	let totalCost = 0;
	let found = false;
	const processedHashes = new Set<string>();
	
	for (const basePath of claudePaths) {
		try {
			// Find session file in projects directory
			const projectsPath = path.join(basePath, "projects");
			if (!fs.existsSync(projectsPath)) continue;
			
			const dirs = fs.readdirSync(projectsPath, { withFileTypes: true });
			for (const dirEntry of dirs) {
				if (!dirEntry.isDirectory()) continue;
				
				const sessionFile = path.join(projectsPath, dirEntry.name, `${sessionId}.jsonl`);
				if (fs.existsSync(sessionFile)) {
					const content = fs.readFileSync(sessionFile, 'utf-8');
					const lines = content.trim().split("\n").filter(line => line);
					
					for (const line of lines) {
						try {
							const entry = JSON.parse(line) as UsageEntry;
							
							// Create unique hash for deduplication
							const messageId = entry.message?.id;
							const requestId = entry.requestId;
							if (messageId && requestId) {
								const uniqueHash = `${messageId}:${requestId}`;
								if (processedHashes.has(uniqueHash)) {
									continue; // Skip duplicate
								}
								processedHashes.add(uniqueHash);
							}
							
							// First check for pre-calculated costUSD
							if (entry.costUSD) {
								totalCost += entry.costUSD;
								found = true;
							}
							// Otherwise calculate from usage data
							else if (entry.message?.usage) {
								const usage = entry.message.usage;
								const modelName = entry.message.model || entry.model;
								const pricing = getModelPricing(modelName);
								
								if (pricing) {
									const cost = calculateCost({
										input: usage.input_tokens,
										output: usage.output_tokens,
										cacheCreation: usage.cache_creation_input_tokens,
										cacheRead: usage.cache_read_input_tokens,
									}, pricing);
									
									if (cost > 0) {
										totalCost += cost;
										found = true;
									}
								}
							}
						} catch {
							// Skip invalid lines
						}
					}
				}
			}
		} catch {
			// Skip inaccessible directories
		}
	}
	
	return found ? { totalCost } : null;
}

// Load today's usage data
async function loadTodayUsageData(): Promise<number> {
	const claudePaths = getClaudePaths();
	const today = new Date().toISOString().split("T")[0];
	let totalCost = 0;
	const processedHashes = new Set<string>();
	
	for (const basePath of claudePaths) {
		try {
			const projectsPath = path.join(basePath, "projects");
			if (!fs.existsSync(projectsPath)) continue;
			
			const dirs = fs.readdirSync(projectsPath, { withFileTypes: true });
			for (const dirEntry of dirs) {
				if (!dirEntry.isDirectory()) continue;
				
				const files = fs.readdirSync(path.join(projectsPath, dirEntry.name), { withFileTypes: true });
				for (const file of files) {
					if (!file.name.endsWith(".jsonl")) continue;
					
					const filePath = path.join(projectsPath, dirEntry.name, file.name);
					const content = fs.readFileSync(filePath, 'utf-8');
					const lines = content.trim().split("\n").filter(line => line);
					
					for (const line of lines) {
						try {
							const entry = JSON.parse(line) as UsageEntry;
							if (entry.timestamp && entry.timestamp.startsWith(today)) {
								// Create unique hash for deduplication
								const messageId = entry.message?.id;
								const requestId = entry.requestId;
								if (messageId && requestId) {
									const uniqueHash = `${messageId}:${requestId}`;
									if (processedHashes.has(uniqueHash)) {
										continue; // Skip duplicate
									}
									processedHashes.add(uniqueHash);
								}
								
								// Check for pre-calculated costUSD
								if (entry.costUSD) {
									totalCost += entry.costUSD;
								}
								// Otherwise calculate from usage data
								else if (entry.message?.usage) {
									const usage = entry.message.usage;
									const modelName = entry.message.model || entry.model;
									const pricing = getModelPricing(modelName);
									
									if (pricing) {
										const cost = calculateCost({
											input: usage.input_tokens,
											output: usage.output_tokens,
											cacheCreation: usage.cache_creation_input_tokens,
											cacheRead: usage.cache_read_input_tokens,
										}, pricing);
										
										totalCost += cost;
									}
								}
							}
						} catch {
							// Skip invalid lines
						}
					}
				}
			}
		} catch {
			// Skip inaccessible directories
		}
	}
	
	return totalCost;
}

// Helper function to floor timestamp to the hour
function floorToHour(timestamp: Date): Date {
	const floored = new Date(timestamp);
	floored.setUTCMinutes(0, 0, 0);
	return floored;
}

// Load active session block
async function loadActiveBlock(): Promise<{ blockInfo: string; burnRateInfo: string, remainingInfo: string }> {
	const claudePaths = getClaudePaths();
	const now = new Date();
	const fiveHoursInMs = 5 * 60 * 60 * 1000;
	
	// Find entries within the last 5 hours
	const recentEntries: UsageEntry[] = [];
	let blockStartTime: Date | null = null;
	let totalCost = 0;
	const processedHashes = new Set<string>();
	
	for (const basePath of claudePaths) {
		try {
			const projectsPath = path.join(basePath, "projects");
			if (!fs.existsSync(projectsPath)) continue;
			
			const dirs = fs.readdirSync(projectsPath, { withFileTypes: true });
			for (const dirEntry of dirs) {
				if (!dirEntry.isDirectory()) continue;
				
				const files = fs.readdirSync(path.join(projectsPath, dirEntry.name), { withFileTypes: true });
				for (const file of files) {
					if (!file.name.endsWith(".jsonl")) continue;
					
					const filePath = path.join(projectsPath, dirEntry.name, file.name);
					const content = fs.readFileSync(filePath, 'utf-8');
					const lines = content.trim().split("\n").filter(line => line);
					
					for (const line of lines) {
						try {
							const entry = JSON.parse(line) as UsageEntry;
							if (entry.timestamp) {
								const entryTime = new Date(entry.timestamp);
								const timeSinceEntry = now.getTime() - entryTime.getTime();
								
								if (timeSinceEntry <= fiveHoursInMs) {
									// Create unique hash for deduplication
									const messageId = entry.message?.id;
									const requestId = entry.requestId;
									if (messageId && requestId) {
										const uniqueHash = `${messageId}:${requestId}`;
										if (processedHashes.has(uniqueHash)) {
											continue; // Skip duplicate
										}
										processedHashes.add(uniqueHash);
									}
									
									recentEntries.push(entry);
									if (!blockStartTime || entryTime < blockStartTime) {
										blockStartTime = entryTime;
									}
									// Check for pre-calculated costUSD
									if (entry.costUSD) {
										totalCost += entry.costUSD;
									}
									// Otherwise calculate from usage data
									else if (entry.message?.usage) {
										const usage = entry.message.usage;
										const modelName = entry.message.model || entry.model;
										const pricing = getModelPricing(modelName);
										
										if (pricing) {
											const cost = calculateCost({
												input: usage.input_tokens,
												output: usage.output_tokens,
												cacheCreation: usage.cache_creation_input_tokens,
												cacheRead: usage.cache_read_input_tokens,
											}, pricing);
											
											totalCost += cost;
										}
									}
								}
							}
						} catch {
							// Skip invalid lines
						}
					}
				}
			}
		} catch {
			// Skip inaccessible directories
		}
	}
	
	if (recentEntries.length === 0 || !blockStartTime) {
		return { blockInfo: "No active block", burnRateInfo: "", remainingInfo: "" };
	}
	
	// Floor the block start time to the hour (same as ccusage)
	blockStartTime = floorToHour(blockStartTime);
	
	// Calculate block end time
	const blockEndTime = new Date(blockStartTime.getTime() + fiveHoursInMs);
	const remaining = Math.round((blockEndTime.getTime() - now.getTime()) / (1000 * 60));
	
	// Calculate burn rate
	const elapsedMinutes = (now.getTime() - blockStartTime.getTime()) / (1000 * 60);
	let burnRateInfo = "";
	
	if (elapsedMinutes > 5) {
		const costPerHour = (totalCost / elapsedMinutes) * 60;
		const costPerHourStr = `${formatCurrency(costPerHour)}/hr`;
		
		// Simple burn rate coloring based on cost
		const coloredBurnRate = costPerHour < 200.0
			? chalk.green(costPerHourStr)
			: costPerHour < 400.0
				? chalk.yellow(costPerHourStr)
				: chalk.red(costPerHourStr);
		
		burnRateInfo = `${coloredBurnRate}`;
	}
	
	const blockInfo = `${formatCurrency(totalCost)} block`;
	const remainingInfo = chalk.magenta(`${formatRemainingTime(remaining)}`);
	return { blockInfo, burnRateInfo, remainingInfo };
}

// Get Git branch name
async function getGitBranch(cwd: string): Promise<string | null> {
	try {
		// Try to read .git/HEAD (fastest method)
		const headPath = path.join(cwd, ".git", "HEAD");
		const content = fs.readFileSync(headPath, 'utf-8');
		const trimmedContent = content.trim();
		
		// Parse ref: refs/heads/branch-name format
		const match = trimmedContent.match(/^ref: refs\/heads\/(.+)$/);
		if (match) {
			return match[1];
		}
		
		// For detached HEAD, return first 7 chars of commit hash
		if (trimmedContent.length >= 7 && !trimmedContent.startsWith("ref:")) {
			return trimmedContent.substring(0, 7);
		}
		
		return null;
	} catch {
		// Not a git repository or .git/HEAD not accessible
		return null;
	}
}

// Calculate context tokens from transcript
async function calculateContextTokens(transcriptPath: string): Promise<string | null> {
	try {
		const content = fs.readFileSync(transcriptPath, 'utf-8');
		// Simple approximation: 1 token ≈ 4 characters
		const estimatedTokens = Math.round(content.length / 4);
		
		// Assume 200k token limit for modern Claude models
		const maxTokens = 200000;
		const percentage = Math.round((estimatedTokens / maxTokens) * 100);
		
		// Color coding
		const color = percentage < 50
			? chalk.green
			: percentage < 80
				? chalk.yellow
				: chalk.red;
		const coloredPercentage = color(`${percentage}%`);
		
		return `${estimatedTokens.toLocaleString()} (${coloredPercentage})`;
	} catch {
		return null;
	}
}

// Main function
async function main() {
	// Read input from stdin
	let input = '';
	
	// For Bun, we need to read stdin differently
	if (process.stdin.isTTY) {
		console.log("❌ No input provided");
		process.exit(1);
	}
	
	// Read from stdin
	for await (const chunk of process.stdin) {
		input += chunk;
	}
	
	const trimmedInput = input.trim();
	
	if (trimmedInput.length === 0) {
		console.log("❌ No input provided");
		process.exit(1);
	}
	
	// Parse input
	let hookData: StatuslineHookJson;
	try {
		const parsed = JSON.parse(trimmedInput);
		hookData = statuslineHookJsonSchema.parse(parsed);
	} catch (error) {
		console.log("❌ Invalid input format:", error instanceof Error ? error.message : String(error));
		process.exit(1);
	}
	
	// Check Claude paths
	const claudePaths = getClaudePaths();
	if (claudePaths.length === 0) {
		console.log("❌ No Claude data directory found");
		process.exit(1);
	}
	
	// Load all data in parallel
	const [sessionData, todayCost, blockData, contextInfo, gitBranch] = await Promise.all([
		loadSessionUsageById(hookData.session_id),
		loadTodayUsageData(),
		loadActiveBlock(),
		calculateContextTokens(hookData.transcript_path),
		getGitBranch(hookData.cwd),
	]);
	
	// Format output
	const currentDir = chalk.green(path.basename(hookData.cwd));
	const colorReset = "\x1b[0m"
	const branchDisplay = gitBranch ? ` ${chalk.cyan(`${gitBranch}`)}` : "";
	const modelName = hookData.model.display_name;
	const isOpus = modelName.toLowerCase().includes("opus");
	const coloredModelName = isOpus ? chalk.white(modelName) : chalk.bold.yellow(modelName);
	const sessionDisplay = sessionData ? formatCurrency(sessionData.totalCost) : "N/A";
	const burnRateInfo = blockData.burnRateInfo ? ` 🔥 ${blockData.burnRateInfo}` : ""
	const remainingInfo = blockData.remainingInfo ? ` ⏰ ${blockData.remainingInfo}` : ""
	const contextInfoStr = contextInfo ? ` ⚖️ ${contextInfo}`: ""
	// Reset color at the beginning to ensure clean output
	const statusLine = `${colorReset}${currentDir}${branchDisplay} 👤 ${coloredModelName}${colorReset}${remainingInfo} 💰 ${formatCurrency(todayCost)} today, ${sessionDisplay} session, ${blockData.blockInfo}${burnRateInfo}${contextInfoStr}`;
	
	console.log(statusLine);
}

// Run main function
await main();