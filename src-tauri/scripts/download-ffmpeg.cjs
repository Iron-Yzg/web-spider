const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');

const ffmpegDir = path.join(__dirname, '../ffmpeg');
const ffmpegMac = path.join(ffmpegDir, 'ffmpeg');

// Create directory if not exists
if (!fs.existsSync(ffmpegDir)) {
    fs.mkdirSync(ffmpegDir, { recursive: true });
}

// Check if FFmpeg already exists
if (fs.existsSync(ffmpegMac)) {
    console.log('FFmpeg already exists, skipping download');
    process.exit(0);
}

console.log('Downloading FFmpeg for macOS...');

// Download FFmpeg from official builds
const ffmpegUrl = 'https://evermeet.cx/ffmpeg/ffmpeg-7.0.2.zip';

const downloadFile = (url, dest) => {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        https.get(url, (response) => {
            if (response.statusCode === 302 || response.statusCode === 301) {
                downloadFile(response.headers.location, dest).then(resolve).catch(reject);
                return;
            }
            response.pipe(file);
            file.on('finish', () => {
                file.close();
                resolve();
            });
        }).on('error', (err) => {
            fs.unlink(dest, () => {});
            reject(err);
        });
    });
};

async function main() {
    try {
        const zipPath = path.join(ffmpegDir, 'ffmpeg.zip');
        await downloadFile(ffmpegUrl, zipPath);
        console.log('Downloaded, extracting...');

        // Extract using system unzip
        execSync(`unzip -o "${zipPath}" -d "${ffmpegDir}"`, { stdio: 'inherit' });

        // Clean up zip
        fs.unlinkSync(zipPath);

        // Make executable
        if (fs.existsSync(ffmpegMac)) {
            fs.chmodSync(ffmpegMac, '755');
            console.log('FFmpeg downloaded and extracted successfully');
        } else {
            console.error('FFmpeg extraction failed');
            process.exit(1);
        }
    } catch (error) {
        console.error('Failed to download FFmpeg:', error.message);
        process.exit(1);
    }
}

main();
