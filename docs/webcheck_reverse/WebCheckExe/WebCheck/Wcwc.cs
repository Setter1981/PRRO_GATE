using System;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Wcwc
{
	private readonly Random Rnd;

	private int rrr;

	private int KeyT;

	public Wcwc()
	{
		Rnd = new Random();
		rrr = 7;
		KeyT = 0;
	}

	public byte Hgb(byte value, bool decoding = false, byte keyS = 0)
	{
		string text = keyS.ToString();
		KeyT = Conversions.ToInteger(text[0].ToString());
		rrr = checked(1 + KeyT);
		return Kod1(value, decoding);
	}

	public byte HGBkey(byte value, bool decoding = false, byte keyS = 0)
	{
		if (decoding)
		{
			rrr = keyS;
		}
		else
		{
			rrr = checked(keyS + 3);
		}
		return Kod1(value, decoding);
	}

	private byte Kod1(byte value, bool decoding = false)
	{
		checked
		{
			if (decoding)
			{
				value = (byte)unchecked((uint)(value + checked((byte)KeyT)));
			}
			string text = value.ToString();
			int num = 0;
			if (!decoding)
			{
				int num2 = Rnd.Next(5);
				if (num2 < 1)
				{
					num2 = 1;
				}
				if (num2 > 2)
				{
					num2 = 2;
				}
				if (num2 == 2)
				{
					num = Rnd.Next(1, 3);
				}
				int num3 = rrr + num;
				for (int i = 0; i <= num3; i++)
				{
					value = (byte)unchecked((uint)(value + 1));
					if (value > 9)
					{
						value = 0;
					}
				}
				if (num2 > 0)
				{
					value = Kod2(value, decoding: false, num);
				}
				if (num2 > 1)
				{
					text = num.ToString() + value;
					value = Conversions.ToByte(text);
				}
				value = (byte)unchecked((uint)(value - checked((byte)KeyT)));
			}
			else
			{
				if (text.Length == 3)
				{
					num = Conversions.ToInteger(text[0].ToString());
					text = Conversions.ToString(text[1]) + Conversions.ToString(text[2]);
				}
				if (text.Length == 2)
				{
					value = Conversions.ToByte(text);
					value = Kod2(value, decoding: true, num);
				}
				int num4 = rrr + num;
				for (int i = 0; i <= num4; i++)
				{
					unchecked
					{
						value = (byte)((value != 0) ? checked((byte)unchecked((uint)(value - 1))) : 9);
					}
				}
			}
			return value;
		}
	}

	private byte Kod2(byte value, bool decoding = false, int typR = 0)
	{
		typR = ((typR != 2) ? 10 : 5);
		if (!decoding)
		{
			string text = Rnd.Next(1, typR).ToString();
			value = Kod3(value, decoding: false, Conversions.ToInteger(text));
			return Conversions.ToByte(text + value);
		}
		string text2 = value.ToString();
		return Kod3(Conversions.ToByte(text2[1].ToString()), decoding: true, Conversions.ToInteger(text2[0].ToString()));
	}

	private byte Kod3(byte value, bool decoding = false, int rot = 0)
	{
		value.ToString();
		if (!decoding)
		{
			checked
			{
				for (int i = 0; i <= rot; i++)
				{
					value = (byte)unchecked((uint)(value + 1));
					if (value > 9)
					{
						value = 0;
					}
				}
			}
		}
		else
		{
			for (int i = 0; i <= rot; i = checked(i + 1))
			{
				value = (byte)((value != 0) ? checked((byte)unchecked((uint)(value - 1))) : 9);
			}
		}
		return value;
	}
}
