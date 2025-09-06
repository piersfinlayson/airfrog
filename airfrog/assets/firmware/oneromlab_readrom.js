async function readRom() {
    try {
        const response = await fetch('/api/firmware/read_rom', { method: 'POST' });
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }
        const data = await response.json();
        if (!data) {
            throw new Error('API returned null data');
        }
        // Define the desired field order
        const fieldOrder = ["Name", "Part Number", "Checksum", "SHA1 Digest"];
        // Format the values
        const formatValue = (key, value) => {
            if (key === "Checksum") {
                return `0x${value.toString(16).padStart(8, '0')}`;
            } else if (key === "SHA1 Digest") {
                return value.map(byte => byte.toString(16).padStart(2, '0')).join('');
            }
            return value;
        };
        let html = '<table class="device-info"><tbody>';
        // First, add the ordered fields that exist
        for (const key of fieldOrder) {
            if (data.hasOwnProperty(key)) {
                const formattedValue = formatValue(key, data[key]);
                html += `<tr><td class="label-col" style="width: 200px;"><strong>${key}</strong></td><td>${formattedValue}</td></tr>`;
            }
        }
        // Then add any remaining fields not in our order list
        for (const [key, value] of Object.entries(data)) {
            if (!fieldOrder.includes(key)) {
                const formattedValue = formatValue(key, value);
                html += `<tr><td class="label-col" style="width: 200px;"><strong>${key}</strong></td><td>${formattedValue}</td></tr>`;
            }
        }
        html += '</tbody></table>';
        document.getElementById('rom-result').innerHTML = html;
    } catch (error) {
        document.getElementById('rom-result').innerHTML = `<p>Error: ${error.message}</p>`;
    }
}
