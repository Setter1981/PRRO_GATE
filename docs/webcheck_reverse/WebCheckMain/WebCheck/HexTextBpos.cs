using System;
using System.Text;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class HexTextBpos
{
	private string[] X;

	private string HS;

	internal int LengthHex
	{
		get
		{
			if (Versioned.IsNumeric((object)X[0]))
			{
				return Conversions.ToInteger(X[0]);
			}
			return 0;
		}
	}

	internal string TegX
	{
		get
		{
			if (e < 1 || e > 99)
			{
				return "";
			}
			return X[e];
		}
	}

	public HexTextBpos()
	{
		X = new string[100];
	}

	internal bool Parsing(string HexStr)
	{
		HexStr = Del06(HexStr);
		HS = HexStr;
		int num = 0;
		checked
		{
			do
			{
				X[num] = "";
				num++;
			}
			while (num <= 99);
			X[0] = HexToText(HexStr.Substring(2, 4), Mirror: false);
			int num2 = (int)Math.Round((double)(HexStr.Length - 6) / 2.0);
			if (!Versioned.IsNumeric((object)X[0]))
			{
				return false;
			}
			if (num2 < Conversions.ToInteger(X[0]))
			{
				return false;
			}
			if (Conversions.ToInteger(X[0]) < 1)
			{
				return false;
			}
			int num3 = 6;
			num = 1;
			do
			{
				num3 = TegH(num3);
				if (num3 == 0)
				{
					break;
				}
				num++;
			}
			while (num <= 99);
			if ((Operators.CompareString(X[1], "13", false) == 0) & (Operators.CompareString(X[31].Trim(), "", false) == 0))
			{
				X[31] = NumToStatus(X[30].Trim());
			}
			return true;
		}
	}

	private string NumToStatus(string e)
	{
		return e switch
		{
			"1" => "Картка прочитана", 
			"2" => "Використовуйте чіп", 
			"3" => "Авторизація з банком", 
			"4" => "Очікування дій касира", 
			"5" => "Друк...", 
			"6" => "Очікування введення ПІН", 
			"7" => "Вийміть картку", 
			"8" => "Вибір програми картки", 
			"9" => "Очікування картки", 
			_ => "---", 
		};
	}

	private string Del06(string e)
	{
		e = e.Trim();
		string result;
		try
		{
			if ((Operators.CompareString(Conversions.ToString(e[0]), "0", false) == 0) & (Operators.CompareString(Conversions.ToString(e[1]), "6", false) == 0))
			{
				e = e.Substring(2, checked(e.Length - 2));
			}
			result = e.Trim();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = e;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private int TegH(int k)
	{
		int result;
		checked
		{
			try
			{
				string text = HexToText(HS.Substring(k, 8));
				k += 8;
				if (!Versioned.IsNumeric((object)text))
				{
					result = 0;
				}
				else
				{
					string text2 = HexToText(HS.Substring(k, 8));
					k += 8;
					if (Versioned.IsNumeric((object)text2))
					{
						string text4;
						if (Conversions.ToInteger(text2) > 0)
						{
							int num = Conversions.ToInteger(text2) * 2;
							string text3 = HS.Substring(k, num);
							if (ASCII_true(text))
							{
								text3 += " ";
							}
							text4 = HexToText(text3);
							k += num;
						}
						else
						{
							text4 = "";
						}
						X[Conversions.ToInteger(text)] = text4;
						goto IL_00c2;
					}
					result = 0;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = 0;
				ProjectData.ClearProjectError();
			}
			goto IL_00c4;
		}
		IL_00c2:
		result = k;
		goto IL_00c4;
		IL_00c4:
		return result;
	}

	private bool ASCII_true(string nT)
	{
		bool result;
		try
		{
			int num = Conversions.ToInteger(nT);
			if (num == 4 || num == 5 || num == 6 || num == 7 || num == 15 || num == 16 || num == 18 || num == 19 || num == 20 || num == 21 || num == 22 || num == 23 || num == 26 || num == 28 || num == 31 || num == 58 || num == 59 || num == 61 || num == 63 || num == 66)
			{
				result = true;
				goto IL_0094;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = true;
			ProjectData.ClearProjectError();
			goto IL_0094;
		}
		result = false;
		goto IL_0094;
		IL_0094:
		return result;
	}

	private string HexToText(string outputHex, bool Mirror = true)
	{
		string result;
		try
		{
			result = ((!Mirror) ? Convert.ToInt64(outputHex, 16) : Convert.ToInt64(MirrorHex(outputHex), 16)).ToString();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			outputHex = outputHex.Trim();
			ProjectData.ClearProjectError();
			goto IL_0041;
		}
		goto IL_00ee;
		IL_0041:
		checked
		{
			try
			{
				int num = (int)Math.Round((double)outputHex.Length / 2.0) - 1;
				byte[] array = new byte[num + 1];
				int num2 = num * 2;
				for (int i = 0; i <= num2; i += 2)
				{
					array[(int)Math.Round((double)i / 2.0)] = Convert.ToByte(outputHex.Substring(i, 2), 16);
					if (array[(int)Math.Round((double)i / 2.0)] == 0)
					{
						array[(int)Math.Round((double)i / 2.0)] = 32;
					}
				}
				result = Encoding.Default.GetString(array);
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				result = "";
				ProjectData.ClearProjectError();
			}
			goto IL_00ee;
		}
		IL_00ee:
		return result;
	}

	private string MirrorHex(string e)
	{
		int length = e.Length;
		if (length < 2)
		{
			return e;
		}
		string text = "";
		checked
		{
			for (int i = length - 2; i >= 0; i += -2)
			{
				text += e.Substring(i, 2);
			}
			return text;
		}
	}

	internal string CreateTag(string eBPOStagcmd, string eBPOSValue)
	{
		string result;
		try
		{
			int num = Conversions.ToInteger(eBPOSValue);
			int num2 = checked((int)Math.Round((double)num.ToString("X8").Length / 2.0));
			result = Conversions.ToInteger(eBPOStagcmd).ToString("X8") + num2.ToString("X8") + num.ToString("X8");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal string CreateTag(int eBPOStagcmd, string eBPOSValue)
	{
		string result;
		try
		{
			int num = Conversions.ToInteger(eBPOSValue);
			int num2 = checked((int)Math.Round((double)num.ToString("X8").Length / 2.0));
			result = eBPOStagcmd.ToString("X8") + num2.ToString("X8") + num.ToString("X8");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal string CreateTag(int eBPOStagcmd, int eBPOSValue)
	{
		string result;
		try
		{
			int num = checked((int)Math.Round((double)eBPOSValue.ToString("X8").Length / 2.0));
			result = eBPOStagcmd.ToString("X8") + num.ToString("X8") + eBPOSValue.ToString("X8");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal string CountLRC(string eHexString)
	{
		checked
		{
			string result;
			try
			{
				int length = eHexString.Length;
				byte[] array = new byte[(int)Math.Round((double)length / 2.0) - 1 + 1];
				for (int i = 0; i < length; i += 2)
				{
					array[(int)Math.Round((double)i / 2.0)] = Convert.ToByte(eHexString.Substring(i, 2), 16);
				}
				byte b = 0;
				int num = array.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					b = unchecked((byte)(b ^ array[i]));
				}
				result = b.ToString("X2");
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = "";
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	internal string CreateTagASCII(int eBPOStagcmd, string eBPOSValue)
	{
		string result;
		try
		{
			eBPOSValue = CreateAsciitiHex(eBPOSValue);
			int num = checked((int)Math.Round((double)eBPOSValue.Length / 2.0));
			result = eBPOStagcmd.ToString("X8") + num.ToString("X8") + eBPOSValue;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string CreateAsciitiHex(string eStrforascii)
	{
		return BitConverter.ToString(Encoding.ASCII.GetBytes(eStrforascii)).Replace("-", string.Empty);
	}
}
