using System;
using System.Runtime.InteropServices;
using System.Text;

namespace WebCheck;

[StructLayout(LayoutKind.Sequential, Size = 1)]
internal struct MonoBankkProtocol
{
	private const byte STX = 2;

	private const byte PROTO_ID = 102;

	private const byte PROTO_VER = 1;

	private byte CalcLRC(byte[] data)
	{
		byte b = 0;
		foreach (byte b2 in data)
		{
			b ^= b2;
		}
		return b;
	}

	public byte[] BuildMessage(string json)
	{
		byte[] bytes = Encoding.ASCII.GetBytes(json);
		int num = bytes.Length;
		if (num > 65535)
		{
			throw new ArgumentException("DATA перевищує 64K");
		}
		checked
		{
			byte[] array = new byte[num + 5 + 1];
			array[0] = 2;
			array[1] = 102;
			array[2] = 1;
			array[3] = (byte)((num >> 8) & 0xFF);
			array[4] = (byte)(num & 0xFF);
			Array.Copy(bytes, 0, array, 5, num);
			array[5 + num] = CalcLRC(bytes);
			return array;
		}
	}

	internal string ParseMessage(byte[] msg, ref bool valid)
	{
		valid = false;
		if (msg == null || msg.Length < 6)
		{
			return null;
		}
		if (msg[0] != 2 || msg[1] != 102 || msg[2] != 1)
		{
			return null;
		}
		int num = (msg[3] << 8) | msg[4];
		checked
		{
			if (msg.Length < 5 + num + 1)
			{
				return null;
			}
			byte[] array = new byte[num - 1 + 1];
			Array.Copy(msg, 5, array, 0, num);
			byte b = msg[5 + num];
			valid = CalcLRC(array) == b;
			return Encoding.UTF8.GetString(array);
		}
	}
}
