import socket
import sys
import time

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
addr = ('localhost', int(sys.argv[1]))
buf = "this is a ping!".encode('utf-8')

print(addr)
print("pinging...", file=sys.stderr)
sock.sendto(buf, ("127.0.0.1", int(sys.argv[1])))

count = 0
while True:
	time.sleep(1)
	buf, raddr = sock.recvfrom(4096)
	if buf and buf.decode("utf-8") == "reply":
		# print(buf.decode("utf-8"), file=sys.stderr)
		print("receive the reply from qemu {}".format(raddr))
		print("test pass!")
		break
	else:
		data = buf.decode("utf-8")
		print("receive the reply from qemu {}, reply: {}".format(raddr,data))
		sock.sendto("this is {}  ping!".format(count).encode('utf-8'), raddr)
		count += 1