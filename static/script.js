const username = document.querySelector("#username");
const joinBtn = document.querySelector("#join-btn");
const textarea = document.querySelector("#chat");
const input = document.querySelector("#input");

let socket = null;

const hasJoined = () => socket !== null;

const onLeave = () => {
    joinBtn.textContent = "Join";
    socket = null;
}

joinBtn.addEventListener("click", (e) => {
    if (hasJoined()) {
        socket.close();
        onLeave();
    } else {
        joinBtn.textContent = "Leave";

        socket = new WebSocket("ws://localhost:3000/ws");
        socket.onopen = () => {
            // Send username as first message
            socket.send(username.value);

            // Activate message sending
            input.onkeydown = (e) => {
                if (e.key == "Enter") {
                    socket.send(input.value);
                    input.value = "";
                }
            }
        }

        socket.onclose = onLeave;

        socket.onmessage = (e) => {
            textarea.value += e.data + "\n";
        }
    }
});