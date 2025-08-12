// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

// Note that this file gets minified by build.rs.
//
// If you add any more externally called functions, add `window` lines at the
// end of the file so the minifier leaves those functions intact.

const API_BASE = '/api';
const MAX_WORD_COUNT = 16384;  // 16K words = 64KB

// Initialize event listeners when DOM is ready
document.addEventListener('DOMContentLoaded', function() {
    // Flash update listeners (existing target_update.js)
    const chooseFileBtn = document.getElementById('chooseFileBtn');
    const fileInput = document.getElementById('firmwareFile');
    const updateButton = document.getElementById('updateButton');
    const eraseButton = document.getElementById('eraseButton');
    const haltButton = document.getElementById('haltButton');
    const resetButton = document.getElementById('resetButton');
    
    if (chooseFileBtn && fileInput && updateButton) {
        chooseFileBtn.addEventListener('click', () => {
            fileInput.click();
        });

        fileInput.addEventListener('change', (e) => {
            const file = e.target.files[0];
            const fileStatus = document.getElementById('fileStatus');
            if (fileStatus) {
                fileStatus.textContent = file ? file.name : 'No file chosen';
            }
            updateButton.disabled = !file;
        });

        updateButton.addEventListener('click', async () => {
            const file = fileInput.files[0];
            if (!file) return;
            await updateFirmware(file);
        });
    }

    if (eraseButton) {
        eraseButton.addEventListener('click', async () => {
            await eraseFlash();
        });
    }

    if (haltButton) {
        haltButton.addEventListener('click', async () => {
            await haltMCU();
        });
    }

    if (resetButton) {
        resetButton.addEventListener('click', async () => {
            await resetMCU();
        });
    }

    // Memory tool listeners
    const chooseMemoryFileBtn = document.getElementById('chooseMemoryFileBtn');
    const memoryFileInput = document.getElementById('memoryFile');
    const updateMemoryButton = document.getElementById('updateMemoryButton');
});

// =============================================================================
// FLASH UPDATE FUNCTIONALITY
// =============================================================================

async function updateFirmware(file) {
    const updateButton = document.getElementById('updateButton');
    if (updateButton) updateButton.disabled = true;
    
    showProgressBar();
    updateProgress(0);
    let response;
    
    try {
        setStatus('Preparing for update...', '');
        
        // Disable keepalive to prevent SWD interference
        response = await fetch(`${API_BASE}/raw/reset`, { method: 'POST' });
        if (!response.ok) throw new Error(`Failed to disable keepalive: ${response.status}`);
        
        // Halt processor before flashing if requested
        const resetBefore = document.getElementById('resetBefore');
        if (resetBefore && resetBefore.checked) {
            await haltProcessor();
        }
        
        // Unlock flash
        response = await fetch(`${API_BASE}/target/flash/unlock`, { method: 'POST' });
        if (!response.ok) throw new Error(`Flash unlock failed: ${response.status}`);

        // Erase all flash if requested
        const eraseBeforeUpdate = document.getElementById('eraseBeforeUpdate');
        if (eraseBeforeUpdate && eraseBeforeUpdate.checked) {
            setStatus('Erasing flash...', '');
            updateProgress(0);
            
            // Start progress simulation (10% per second, cap at 90%)
            let eraseProgress = 0;
            const eraseInterval = setInterval(() => {
                eraseProgress += 10;
                if (eraseProgress <= 90) {
                    updateProgress(eraseProgress);
                }
            }, 1000);
            
            try {
                response = await fetch(`${API_BASE}/target/flash/erase-all`, { method: 'POST' });
                if (!response.ok) throw new Error(`Flash erase failed: ${response.status}`);
            } finally {
                clearInterval(eraseInterval);
                updateProgress(100); // Complete the erase progress
            }
        }

        // Read file as array buffer
        const buffer = await file.arrayBuffer();
        const bytes = new Uint8Array(buffer);
        
        // Flash in 1KB chunks (256 words)
        const chunkSize = 1024;
        const totalChunks = Math.ceil(bytes.length / chunkSize);
        let baseAddr = 0x08000000; // Typical STM32 flash start
        
        updateProgress(0);

        for (let i = 0; i < totalChunks; i++) {
            const start = i * chunkSize;
            const end = Math.min(start + chunkSize, bytes.length);
            const chunk = bytes.slice(start, end);
            
            // Convert bytes to 32-bit words (little endian)
            const words = [];
            for (let j = 0; j < chunk.length; j += 4) {
                const word = (chunk[j] || 0) | 
                            ((chunk[j+1] || 0) << 8) | 
                            ((chunk[j+2] || 0) << 16) | 
                            ((chunk[j+3] || 0) << 24);
                // Ensure unsigned 32-bit value
                const unsignedWord = word >>> 0;
                words.push(`0x${unsignedWord.toString(16).padStart(8, '0').toUpperCase()}`);
            }
            
            // Flash this chunk
            const addr = baseAddr + start;
            response = await fetch(`${API_BASE}/target/flash/bulk/write/0x${addr.toString(16)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ data: words })
            });
            if (!response.ok) throw new Error(`Flash write failed at 0x${addr.toString(16)}: ${response.status}`);
            
            // Update progress
            const progress = Math.round((i + 1) / totalChunks * 100);
            updateProgress(progress);
            setStatus(`Flashing... ${progress}%`, '');
        }
        setStatus('Flashing succeeded', '');

        // Lock flash
        response = await fetch(`${API_BASE}/target/flash/lock`, { method: 'POST' });
        if (!response.ok) throw new Error(`Flash lock failed: ${response.status}`);

        // Verify firmware if requested
        const verifyAfterUpdate = document.getElementById('verifyAfterUpdate');
        if (verifyAfterUpdate && verifyAfterUpdate.checked) {
            updateProgress(0);
            
            for (let i = 0; i < totalChunks; i++) {
                const start = i * chunkSize;
                const end = Math.min(start + chunkSize, bytes.length);
                const originalChunk = bytes.slice(start, end);
                
                // Read back this chunk from flash
                const addr = baseAddr + start;
                const readCount = Math.ceil((end - start) / 4); // words to read
                response = await fetch(`${API_BASE}/target/memory/bulk/read/0x${addr.toString(16)}`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ count: readCount })
                });
                if (!response.ok) throw new Error(`Verify read failed at 0x${addr.toString(16)}: ${response.status}`);
                
                const readData = await response.json();
                
                // Convert read words back to bytes for comparison
                const readBytes = new Uint8Array((end - start));
                for (let j = 0; j < readData.data.length; j++) {
                    const word = parseInt(readData.data[j], 16);
                    const wordStart = j * 4;
                    if (wordStart < readBytes.length) readBytes[wordStart] = word & 0xFF;
                    if (wordStart + 1 < readBytes.length) readBytes[wordStart + 1] = (word >> 8) & 0xFF;
                    if (wordStart + 2 < readBytes.length) readBytes[wordStart + 2] = (word >> 16) & 0xFF;
                    if (wordStart + 3 < readBytes.length) readBytes[wordStart + 3] = (word >> 24) & 0xFF;
                }
                
                // Compare bytes
                for (let j = 0; j < originalChunk.length; j++) {
                    if (originalChunk[j] !== readBytes[j]) {
                        throw new Error(`Verification failed at offset 0x${(start + j).toString(16)}: expected 0x${originalChunk[j].toString(16)}, got 0x${readBytes[j].toString(16)}`);
                    }
                }
                
                // Update verification progress
                const progress = Math.round((i + 1) / totalChunks * 100);
                updateProgress(progress);
                setStatus(`Verifying... ${progress}%`, '');
            }
            
            setStatus('Verification passed', '');
        }

        // Reset after flashing if requested
        const resetAfter = document.getElementById('resetAfter');
        if (resetAfter && resetAfter.checked) {
            await resetProcessor();
            
            // Re-enable keepalive only if resetting
            response = await fetch(`${API_BASE}/target/reset`, { method: 'POST' });
            if (!response.ok) throw new Error(`Failed to re-enable keepalive: ${response.status}`);
        }
        
        // Check final processor state
        const processorState = await checkProcessorState();
        setStatus(`Firmware update complete: ${processorState}`, 'ok');
        
    } catch (error) {
        setStatus(`Update failed: ${error.message}`, 'error');
    } finally {
        if (updateButton) updateButton.disabled = false;
    }
}

async function haltProcessor() {
    setStatus('Halting processor...', '');
    
    // Set TAR to DHCSR address
    let response = await fetch(`${API_BASE}/raw/ap/write/0x0/0x4`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: "0xE000EDF0" })
    });
    if (!response.ok) throw new Error(`Failed to set TAR for halt: ${response.status}`);
    
    // Write C_HALT | C_DEBUGEN to halt processor
    response = await fetch(`${API_BASE}/raw/ap/write/0x0/0xC`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: "0xA05F0003" })
    });
    if (!response.ok) throw new Error(`Failed to halt processor: ${response.status}`);
}

async function resetProcessor() {
    setStatus('Resetting processor...', '');
    
    // Set TAR to AIRCR address
    let response = await fetch(`${API_BASE}/raw/ap/write/0x0/0x4`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: "0xE000ED0C" })
    });
    if (!response.ok) throw new Error(`Failed to set TAR for reset: ${response.status}`);
    
    // Write SYSRESETREQ to reset processor
    response = await fetch(`${API_BASE}/raw/ap/write/0x0/0xC`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: "0x05FA0004" })
    });
    if (!response.ok) throw new Error(`Failed to reset processor: ${response.status}`);
}

async function checkProcessorState() {
    // Set TAR to DHCSR address
    let response = await fetch(`${API_BASE}/raw/ap/write/0x0/0x4`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ data: "0xE000EDF0" })
    });
    if (!response.ok) throw new Error(`Failed to set TAR for state check: ${response.status}`);
    
    // Read DHCSR contents
    response = await fetch(`${API_BASE}/raw/ap/read/0x0/0xC`, { method: 'GET' });
    if (!response.ok) throw new Error(`Failed to read processor state: ${response.status}`);
    
    const data = await response.json();
    const dhcsr = parseInt(data.data, 16);
    
    // Check S_HALT bit (bit 17 = 0x20000)
    return (dhcsr & 0x20000) ? "MCU halted" : "MCU running";
}

async function haltMCU() {
    const haltButton = document.getElementById('haltButton');
    if (haltButton) haltButton.disabled = true;
    
    try {
        // Disable keepalive to prevent SWD interference
        let response = await fetch(`${API_BASE}/raw/reset`, { method: 'POST' });
        if (!response.ok) throw new Error(`Failed to disable keepalive: ${response.status}`);
        
        await haltProcessor();
        
        // Check final processor state (don't re-enable keepalive - want it to stay halted)
        const processorState = await checkProcessorState();
        setStatus(`MCU halt complete: ${processorState}`, 'ok');
        
    } catch (error) {
        setStatus(`Halt failed: ${error.message}`, 'error');
    } finally {
        if (haltButton) haltButton.disabled = false;
    }
}

async function resetMCU() {
    const resetButton = document.getElementById('resetButton');
    if (resetButton) resetButton.disabled = true;
    
    try {
        // Disable keepalive to prevent SWD interference
        let response = await fetch(`${API_BASE}/raw/reset`, { method: 'POST' });
        if (!response.ok) throw new Error(`Failed to disable keepalive: ${response.status}`);
        
        await resetProcessor();
        
        // Re-enable keepalive after reset so MCU runs normally
        response = await fetch(`${API_BASE}/target/reset`, { method: 'POST' });
        if (!response.ok) throw new Error(`Failed to re-enable keepalive: ${response.status}`);
        
        // Check final processor state
        const processorState = await checkProcessorState();
        setStatus(`MCU reset complete: ${processorState}`, 'ok');
        
    } catch (error) {
        setStatus(`Reset failed: ${error.message}`, 'error');
    } finally {
        if (resetButton) resetButton.disabled = false;
    }
}

async function eraseFlash() {
    const eraseButton = document.getElementById('eraseButton');
    if (eraseButton) eraseButton.disabled = true;
    
    showProgressBar();
    updateProgress(0);
    let response;
    
    try {
        setStatus('Preparing to erase flash...', '');
        
        // Disable keepalive to prevent SWD interference
        response = await fetch(`${API_BASE}/raw/reset`, { method: 'POST' });
        if (!response.ok) throw new Error(`Failed to disable keepalive: ${response.status}`);
        
        // Halt processor before erasing if requested
        const resetBefore = document.getElementById('resetBefore');
        if (resetBefore && resetBefore.checked) {
            await haltProcessor();
        }
        
        updateProgress(0);
        
        // Unlock flash
        response = await fetch(`${API_BASE}/target/flash/unlock`, { method: 'POST' });
        if (!response.ok) throw new Error(`Flash unlock failed: ${response.status}`);
        
        // Erase all flash
        setStatus('Erasing flash...', '');
        updateProgress(0);
        
        // Start progress simulation (10% per second, cap at 90%)
        let eraseProgress = 0;
        const eraseInterval = setInterval(() => {
            eraseProgress += 10;
            if (eraseProgress <= 90) {
                updateProgress(eraseProgress);
            }
        }, 1000);
        
        try {
            response = await fetch(`${API_BASE}/target/flash/erase-all`, { method: 'POST' });
            if (!response.ok) throw new Error(`Flash erase failed: ${response.status}`);
        } finally {
            clearInterval(eraseInterval);
        }
        
        // Lock flash
        response = await fetch(`${API_BASE}/target/flash/lock`, { method: 'POST' });
        if (!response.ok) throw new Error(`Flash lock failed: ${response.status}`);
        
        // Reset after erasing if requested
        const resetAfter = document.getElementById('resetAfter');
        if (resetAfter && resetAfter.checked) {
            await resetProcessor();
            
            // Re-enable keepalive only if resetting
            response = await fetch(`${API_BASE}/target/reset`, { method: 'POST' });
            if (!response.ok) throw new Error(`Failed to re-enable keepalive: ${response.status}`);
        }
        
        updateProgress(100);
        
        // Check final processor state
        const processorState = await checkProcessorState();
        setStatus(`Flash erase complete: ${processorState}`, 'ok');
        
    } catch (error) {
        setStatus(`Erase failed: ${error.message}`, 'error');
    } finally {
        if (eraseButton) eraseButton.disabled = false;
    }
}

// =============================================================================
// MEMORY TOOL FUNCTIONALITY
// =============================================================================

function showTab(tabName, event) {
    document.getElementById('viewer-tab').style.display = 'none';
    document.getElementById('writer-tab').style.display = 'none';
    document.getElementById('registers-tab').style.display = 'none';
    
    document.getElementById(tabName + '-tab').style.display = 'block';
    
    document.querySelectorAll('.tab').forEach(tab => tab.classList.remove('active'));
    event.target.classList.add('active');
}

// Address setters
function setViewerAddress(addr) {
    document.getElementById('viewer-address').value = addr;
}

function setWriterAddress(addr) {
    document.getElementById('writer-address').value = addr;
}

function setViewerStatus(message, type) {
    const viewerStatus = document.getElementById('viewer-status');
    if (viewerStatus) {
        viewerStatus.textContent = message;
        viewerStatus.className = `status-message ${type ? 'status ' + type : ''}`;
    }
}

async function readMemory() {
    const address = document.getElementById('viewer-address').value;
    const count = document.getElementById('viewer-count').value;
    hideViewerProgressBar();

    if (!validateHexAddress(address)) {
        setViewerStatus('Invalid hex address format', 'error');
        document.getElementById('viewer-results').innerHTML = '';
        return;
    }
    
    const countNum = parseInt(count);
    if (isNaN(countNum) || countNum < 1 || countNum > MAX_WORD_COUNT) {
        setViewerStatus('Count must be 1-${MAX_WORD_COUNT}', 'error');
        document.getElementById('viewer-results').innerHTML = ''; 
        return;
    }

    const showProgress = countNum > 256;
    if (showProgress) {
        showViewerProgressBar();
        updateViewerProgress(0);
    }

    const chunks = Math.ceil(countNum / 256);
    const allData = [];
    let errorMessage = null;

    setViewerStatus('Reading memory...', '');

    for (let i = 0; i < chunks; i++) {
        const chunkCount = Math.min(256, countNum - (i * 256));
        const chunkAddr = parseInt(address, 16) + (i * 256 * 4);
        
        try {
            const response = await fetch(`${API_BASE}/target/memory/bulk/read/0x${chunkAddr.toString(16)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ count: chunkCount })
            });
            
            const data = await response.json();
            if (data.error) {
                errorMessage = `Read failed at 0x${chunkAddr.toString(16)}: ${formatError(data.error)}`;
                break;
            }
            
            allData.push(...data.data);
            
            if (showProgress) {
                const progress = Math.round((i + 1) / chunks * 100);
                updateViewerProgress(progress);
                setViewerStatus(`Reading... ${progress}%`, '');
            }
        } catch (e) {
            errorMessage = `Network error: ${e.message}`;
            break;
        }
    }

    // Handle results
    if (allData.length > 0) {
        displayHexData(address, allData);
    }

    if (errorMessage) {
        const suffix = allData.length > 0 ? ` (${allData.length} words read)` : '';
        setViewerStatus(errorMessage + suffix, 'error');
        if (allData.length === 0) {
            document.getElementById('viewer-results').innerHTML = '';
        }
    } else {
        setViewerStatus(`Read complete: ${countNum} words from ${address.toUpperCase()}`, 'ok');
    }
}

// Single word reading (writer tab)
async function readWord() {
    const address = document.getElementById('writer-address').value;
    
    if (!validateHexAddress(address)) {
        setSingleWordStatus('Invalid hex address format', 'error');
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/target/memory/read/${address}`);
        const data = await response.json();
        
        if (data.error) {
            setSingleWordStatus(`Read failed: ${formatError(data.error)}`, 'error');
            return;
        }
        
        document.getElementById('writer-value').value = data.data;
        setSingleWordStatus(`Read successful: ${data.data} from ${address.toUpperCase()}`, 'ok');
    } catch (e) {
        setSingleWordStatus(`Network error: ${e.message}`, 'error');
    }
}

// Single word writing (writer tab)
async function writeWord() {
    const address = document.getElementById('writer-address').value;
    const value = document.getElementById('writer-value').value;
    
    if (!validateHexAddress(address)) {
        setSingleWordStatus('Invalid hex address format', 'error');
        return;
    }
    
    if (!value || !validateHexAddress(value)) {
        setSingleWordStatus('Invalid hex value format', 'error');
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/target/memory/write/${address}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ data: value })
        });
        
        const data = await response.json();
        if (data.error) {
            setSingleWordStatus(`Write failed: ${formatError(data.error)}`, 'error');
            return;
        }
        
        setSingleWordStatus(`Write successful: ${value.toUpperCase()} at ${address.toUpperCase()}`, 'ok');
    } catch (e) {
        setSingleWordStatus(`Network error: ${e.message}`, 'error');
    }
}

// Bulk memory operations
async function updateMemoryFromFile(file) {
    const updateMemoryButton = document.getElementById('updateMemoryButton');
    if (updateMemoryButton) updateMemoryButton.disabled = true;
    
    const showProgress = document.getElementById('showProgress');
    if (showProgress && showProgress.checked) {
        showProgressBar();
    }
    updateProgress(0);
    
    try {
        const startAddr = document.getElementById('bulk-address').value;
        const fileFormat = document.getElementById('file-format').value;
        
        if (!validateHexAddress(startAddr)) {
            throw new Error('Invalid start address format');
        }
        
        setWriterStatus('Reading file...', '');
        
        // Parse file based on format
        let data;
        let baseAddress = parseInt(startAddr, 16);
        
        if (fileFormat === 'binary') {
            data = await parseBinaryFile(file);
        } else {
            const result = await parseIntelHexFile(file);
            data = result.data;
            if (result.baseAddress !== null) {
                baseAddress = result.baseAddress;
            }
        }
        
        const sizeKB = (data.length / 1024).toFixed(1);
        const endAddr = baseAddress + data.length - 1;
        setWriterStatus(`Writing ${sizeKB}KB to 0x${baseAddress.toString(16).toUpperCase()}-0x${endAddr.toString(16).toUpperCase()}`, '');
        
        // Convert bytes to 32-bit words
        const words = [];
        for (let i = 0; i < data.length; i += 4) {
            const word = (data[i] || 0) | 
                        ((data[i+1] || 0) << 8) | 
                        ((data[i+2] || 0) << 16) | 
                        ((data[i+3] || 0) << 24);
            const unsignedWord = word >>> 0;
            words.push(`0x${unsignedWord.toString(16).padStart(8, '0').toUpperCase()}`);
        }
        
        // Write in chunks
        const totalChunks = Math.ceil(words.length / 256); // 256 words per chunk
        
        for (let i = 0; i < totalChunks; i++) {
            const start = i * 256;
            const end = Math.min(start + 256, words.length);
            const chunk = words.slice(start, end);
            
            const addr = baseAddress + (start * 4);
            const response = await fetch(`${API_BASE}/target/memory/bulk/write/0x${addr.toString(16)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ data: chunk })
            });
            
            if (!response.ok) {
                throw new Error(`Memory write failed at 0x${addr.toString(16)}: ${response.status}`);
            }
            
            const progress = Math.round((i + 1) / totalChunks * 100);
            updateProgress(progress);
            setWriterStatus(`Writing... ${progress}%`, '');
        }
        
        setWriterStatus(`Memory write complete: ${sizeKB}KB written to 0x${baseAddress.toString(16).toUpperCase()}-0x${endAddr.toString(16).toUpperCase()}`, 'ok');
        
        // Verify if requested
        const verifyAfterWrite = document.getElementById('verifyAfterWrite');
        if (verifyAfterWrite && verifyAfterWrite.checked) {
            await verifyMemory(baseAddress, data);
        }
        
    } catch (error) {
        setWriterStatus(`Update failed: ${error.message}`, 'error');
    } finally {
        if (updateMemoryButton) updateMemoryButton.disabled = false;
    }
}

async function verifyMemory(baseAddress, originalData) {
    setWriterStatus('Verifying...', '');
    updateProgress(0);
    
    const chunkSize = 1024; // bytes
    const totalChunks = Math.ceil(originalData.length / chunkSize);
    
    for (let i = 0; i < totalChunks; i++) {
        const start = i * chunkSize;
        const end = Math.min(start + chunkSize, originalData.length);
        const originalChunk = originalData.slice(start, end);
        
        const addr = baseAddress + start;
        const readCount = Math.ceil((end - start) / 4);
        
        const response = await fetch(`${API_BASE}/target/memory/bulk/read/0x${addr.toString(16)}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ count: readCount })
        });
        
        if (!response.ok) {
            throw new Error(`Verify read failed at 0x${addr.toString(16)}: ${response.status}`);
        }
        
        const readData = await response.json();
        
        // Convert read words back to bytes
        const readBytes = new Uint8Array(end - start);
        for (let j = 0; j < readData.data.length; j++) {
            const word = parseInt(readData.data[j], 16);
            const wordStart = j * 4;
            if (wordStart < readBytes.length) readBytes[wordStart] = word & 0xFF;
            if (wordStart + 1 < readBytes.length) readBytes[wordStart + 1] = (word >> 8) & 0xFF;
            if (wordStart + 2 < readBytes.length) readBytes[wordStart + 2] = (word >> 16) & 0xFF;
            if (wordStart + 3 < readBytes.length) readBytes[wordStart + 3] = (word >> 24) & 0xFF;
        }
        
        // Compare bytes
        for (let j = 0; j < originalChunk.length; j++) {
            if (originalChunk[j] !== readBytes[j]) {
                throw new Error(`Verification failed at offset 0x${(start + j).toString(16)}: expected 0x${originalChunk[j].toString(16)}, got 0x${readBytes[j].toString(16)}`);
            }
        }
        
        const progress = Math.round((i + 1) / totalChunks * 100);
        updateProgress(progress);
        setWriterStatus(`Verifying... ${progress}%`, '');
    }
    
    setWriterStatus(`Verification passed: ${originalData.length} bytes verified successfully`, 'ok');
}

async function fillMemory() {
    const address = document.getElementById('bulk-address').value;
    const pattern = prompt('Enter fill pattern (hex, e.g., 0xDEADBEEF):');
    const sizeStr = prompt('Enter size in bytes (e.g., 1024):');
    
    if (!pattern || !sizeStr) return;
    if (!validateHexAddress(address) || !validateHexAddress(pattern)) {
        setWriterStatus('Invalid address or pattern format', 'error');
        return;
    }
    
    const size = parseInt(sizeStr);
    if (isNaN(size) || size <= 0) {
        setWriterStatus('Invalid size', 'error');
        return;
    }
    
    try {
        const patternWord = parseInt(pattern, 16);
        const wordCount = Math.ceil(size / 4);
        const totalChunks = Math.ceil(wordCount / 256);

        for (let i = 0; i < totalChunks; i++) {
            const chunkSize = Math.min(256, wordCount - (i * 256));
            const chunkAddr = parseInt(address, 16) + (i * 256 * 4);
            const words = Array(chunkSize).fill(pattern); // or '0x00000000' for clear
            
            const response = await fetch(`${API_BASE}/target/memory/bulk/write/0x${chunkAddr.toString(16)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ data: words })
            });
            
            if (!response.ok) {
                throw new Error(`Operation failed at 0x${chunkAddr.toString(16)}: ${response.status}`);
            }
        }
        
        setWriterStatus(`Fill successful: ${size} bytes filled with ${pattern} at ${address.toUpperCase()}`, 'ok');
    } catch (error) {
        setWriterStatus(`Fill failed: ${error.message}`, 'error');
    }
}

async function clearMemory() {
    const address = document.getElementById('bulk-address').value;
    const sizeStr = prompt('Enter size in bytes to clear (e.g., 1024):');
    
    if (!sizeStr) return;
    if (!validateHexAddress(address)) {
        setWriterStatus('Invalid address format', 'error');
        return;
    }
    
    const size = parseInt(sizeStr);
    if (isNaN(size) || size <= 0) {
        setWriterStatus('Invalid size', 'error');
        return;
    }
    
    try {
        const wordCount = Math.ceil(size / 4);
        const totalChunks = Math.ceil(wordCount / 256);

        for (let i = 0; i < totalChunks; i++) {
            const chunkSize = Math.min(256, wordCount - (i * 256));
            const chunkAddr = parseInt(address, 16) + (i * 256 * 4);
            const words = Array(chunkSize).fill(0x00000000);
            
            const response = await fetch(`${API_BASE}/target/memory/bulk/write/0x${chunkAddr.toString(16)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ data: words })
            });
            
            if (!response.ok) {
                throw new Error(`Operation failed at 0x${chunkAddr.toString(16)}: ${response.status}`);
            }
        }
        
        setWriterStatus(`Clear successful: ${size} bytes cleared at ${address.toUpperCase()}`, 'ok');
    } catch (error) {
        setWriterStatus(`Clear failed: ${error.message}`, 'error');
    }
}

// File parsing
async function parseBinaryFile(file) {
    const buffer = await file.arrayBuffer();
    return new Uint8Array(buffer);
}

async function parseIntelHexFile(file) {
    const text = await file.text();
    const lines = text.split('\n').map(line => line.trim()).filter(line => line.length > 0);
    
    const data = [];
    let baseAddress = null;
    let extendedAddress = 0;
    
    for (const line of lines) {
        if (!line.startsWith(':')) continue;
        
        const byteCount = parseInt(line.substr(1, 2), 16);
        const address = parseInt(line.substr(3, 4), 16);
        const recordType = parseInt(line.substr(7, 2), 16);
        
        if (recordType === 0) { // Data record
            const fullAddress = extendedAddress + address;
            if (baseAddress === null) {
                baseAddress = fullAddress;
            }
            
            // Ensure data array is large enough
            const endAddr = fullAddress + byteCount;
            while (data.length < endAddr - baseAddress) {
                data.push(0xFF);
            }
            
            // Copy data
            for (let i = 0; i < byteCount; i++) {
                const byte = parseInt(line.substr(9 + i * 2, 2), 16);
                data[fullAddress - baseAddress + i] = byte;
            }
        } else if (recordType === 4) { // Extended Linear Address
            extendedAddress = parseInt(line.substr(9, 4), 16) << 16;
        } else if (recordType === 1) { // End of file
            break;
        }
    }
    
    return { data: new Uint8Array(data), baseAddress };
}

// Register operations (from browser.js)
async function readRegister() {
    const type = document.getElementById('reg-type').value;
    const register = document.getElementById('register').value;
    const apIndex = document.getElementById('ap-index').value;
    
    if (!validateHexAddress(register)) {
        alert('Invalid register address format');
        return;
    }
    
    if (!validateHexAddress(apIndex)) {
        alert('Invalid AP index format');
        return;
    }

    const url = type === 'dp' 
        ? `${API_BASE}/raw/dp/read/${register}`
        : `${API_BASE}/raw/ap/read/${apIndex}/${register}`;
    
    try {
        const response = await fetch(url);
        const data = await response.json();
        alert(data.error ? 'Error: ' + JSON.stringify(data.error) : 'Value: ' + data.data);
    } catch (e) {
        alert('Network error: ' + e.message);
    }
}

async function writeRegister() {
    const type = document.getElementById('reg-type').value;
    const register = document.getElementById('register').value;
    const apIndex = document.getElementById('ap-index').value;

    if (!validateHexAddress(register)) {
        alert('Invalid register address format');
        return;
    }
    
    if (!validateHexAddress(apIndex)) {
        alert('Invalid AP index format');
        return;
    }
    
    const value = prompt('Enter value to write (hex):');
    if (!value) return;
    if (!validateHexAddress(value)) {
        alert('Invalid hex value format');
        return;
    }

    const url = type === 'dp' 
        ? `${API_BASE}/raw/dp/write/${register}`
        : `${API_BASE}/raw/ap/write/${apIndex}/${register}`;
    
    try {
        const response = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ data: value })
        });
        
        const data = await response.json();
        alert(data.error ? 'Error: ' + JSON.stringify(data.error) : 'Write successful');
    } catch (e) {
        alert('Network error: ' + e.message);
    }
}

// =============================================================================
// SHARED UTILITY FUNCTIONS
// =============================================================================

function validateHexAddress(addr) {
    if (!addr || addr.trim() === '') return false;
    const cleanAddr = addr.replace(/^0x/i, '');
    return /^[0-9A-Fa-f]{1,8}$/.test(cleanAddr);
}

function updateProgress(percent) {
    const progressFill = document.querySelector('.progress-fill');
    const progressText = document.querySelector('.progress-text');
    if (progressFill && progressText) {
        progressFill.style.width = `${percent}%`;
        progressText.textContent = `${percent}%`;
    }
}

function showProgressBar() {
    const progressSection = document.querySelector('.progress-section');
    if (progressSection) {
        progressSection.style.display = 'block';
    }
}

function showViewerProgressBar() {
    const progressSection = document.querySelector('.viewer-progress-section');
    if (progressSection) {
        progressSection.style.display = 'block';
    }
}

function hideViewerProgressBar() {
    const progressSection = document.querySelector('.viewer-progress-section');
    if (progressSection) {
        progressSection.style.display = 'none';
    }
}

function updateViewerProgress(percent) {
    const viewerSection = document.querySelector('.viewer-progress-section');
    if (viewerSection) {
        const progressFill = viewerSection.querySelector('.progress-fill');
        const progressText = viewerSection.querySelector('.progress-text');
        if (progressFill && progressText) {
            progressFill.style.width = `${percent}%`;
            progressText.textContent = `${percent}%`;
        }
    }
}

function setStatus(message, type) {
    const statusMessage = document.getElementById('statusMessage');
    if (statusMessage) {
        statusMessage.textContent = message;
        statusMessage.className = `status-message ${type ? 'status ' + type : ''}`;
    }
}

function setSingleWordStatus(message, type) {
    const singleWordStatus = document.getElementById('single-word-status');
    if (singleWordStatus) {
        singleWordStatus.textContent = message;
        singleWordStatus.className = `status-message ${type ? 'status ' + type : ''}`;
    }
}

function setWriterStatus(message, type) {
    const writerStatus = document.getElementById('writer-status');
    if (writerStatus) {
        writerStatus.textContent = message;
        writerStatus.className = `status-message ${type ? 'status ' + type : ''}`;
    }
}
function displayHexData(startAddr, dataArray) {
    const results = document.getElementById('viewer-results');
    let html = '<div class="hex-display">';
    
    const baseAddr = parseInt(startAddr, 16);
    
    for (let i = 0; i < dataArray.length; i += 4) {
        const addr = (baseAddr + i * 4).toString(16).toUpperCase().padStart(8, '0');
        const hexRow = dataArray.slice(i, i + 4).map(val => {
            const hex = parseInt(val, 16);
            return [
                (hex & 0xFF).toString(16).padStart(2, '0'),
                ((hex >> 8) & 0xFF).toString(16).padStart(2, '0'),
                ((hex >> 16) & 0xFF).toString(16).padStart(2, '0'),
                ((hex >> 24) & 0xFF).toString(16).padStart(2, '0')
            ].join(' ');
        }).join(' ');
        
        const asciiRow = dataArray.slice(i, i + 4).map(val => {
            const hex = parseInt(val, 16);
            return [hex & 0xFF, (hex >> 8) & 0xFF, (hex >> 16) & 0xFF, (hex >> 24) & 0xFF]
                .map(b => {
                    if (b >= 32 && b <= 126) {
                        const char = String.fromCharCode(b);
                        return char.replace(/&/g, '&amp;')
                                .replace(/</g, '&lt;')
                                .replace(/>/g, '&gt;')
                                .replace(/"/g, '&quot;')
                                .replace(/'/g, '&#39;');
                    }
                    return '.';
                })
                .join('');
        }).join('');
        
        html += `
            <div class="address-col">${addr}:</div>
            <div class="hex-col">${hexRow.toUpperCase()}</div>
            <div class="ascii-col">${asciiRow}</div>
        `;
    }
    
    html += '</div>';
    results.innerHTML = html;
}

function displayViewerError(error) {
    const results = document.getElementById('viewer-results');
    const message = formatError(error);
    results.innerHTML = `<div class="error">Error: ${message}</div>`;
}

function formatError(error) {
    if (error.error) {
        return error.error;
    } else if (error.swd && error.swd.kind) {
        return `SWD ${error.swd.kind}${error.swd.detail ? ': ' + error.swd.detail : ''}`;
    } else if (error.airfrog && error.airfrog.kind) {
        return `${error.airfrog.kind}${error.airfrog.detail ? ': ' + error.airfrog.detail : ''}`;
    } else {
        return JSON.stringify(error);
    }
}

function handleMemoryFileChange(input) {
    const file = input.files[0];
    const memoryFileStatus = document.getElementById('memoryFileStatus');
    const updateMemoryButton = document.getElementById('updateMemoryButton');
    
    if (memoryFileStatus) {
        memoryFileStatus.textContent = file ? file.name : 'No file chosen';
    }
    if (updateMemoryButton) {
        updateMemoryButton.disabled = !file;
    }
}

function handleMemoryUpdate() {
    const memoryFile = document.getElementById('memoryFile');
    const file = memoryFile ? memoryFile.files[0] : null;
    if (!file) return;
    
    // Call your existing function
    updateMemoryFromFile(file);
}

window.handleMemoryFileChange = handleMemoryFileChange;
window.handleMemoryUpdate = handleMemoryUpdate;
window.showTab = showTab;
window.readMemory = readMemory;
window.writeWord = writeWord;
window.readWord = readWord;
window.setViewerAddress = setViewerAddress;
window.setWriterAddress = setWriterAddress;
window.fillMemory = fillMemory;
window.clearMemory = clearMemory;
window.readRegister = readRegister;
window.writeRegister = writeRegister;
window.updateFirmware = updateFirmware;
window.eraseFlash = eraseFlash;
window.haltMCU = haltMCU;
window.resetMCU = resetMCU;