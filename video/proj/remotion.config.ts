import { Config } from "@remotion/cli/config";
// 本机铁律：webpack only — Config.setRspack(true) 会静默崩（owner 本地约束）
Config.setVideoImageFormat("jpeg");
Config.setOverwriteOutput(true);
