using System;
using System.Runtime.InteropServices;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[StructLayout(LayoutKind.Sequential, Size = 1)]
internal struct NP
{
	internal bool FileArchive(ref string f, ref string t)
	{
		string text = f;
		string text2 = t;
		if (text.Length < 10)
		{
			return false;
		}
		if (text2.Length < 8)
		{
			return false;
		}
		if (!Versioned.IsNumeric(text))
		{
			return false;
		}
		if (Operators.CompareString(text, text2, TextCompare: false) == 0)
		{
			int num;
			try
			{
				num = checked(Conversions.ToInteger(text) * Conversions.ToInteger(text2));
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				num = 0;
				ProjectData.ClearProjectError();
			}
			text2 = num.ToString();
		}
		string text3 = NamePassFile(text, text2);
		string text4 = Sum2(text) + NamePassFile(text2, text) + Sum2(text2);
		f = text3;
		t = text4;
		return true;
	}

	private string CharToNum(string e)
	{
		if (Versioned.IsNumeric(e))
		{
			return e;
		}
		string text = "";
		checked
		{
			int num = e.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				string text2 = e[i].ToString();
				text = ((!Versioned.IsNumeric(text2)) ? (text + i) : (text + text2));
			}
			return text;
		}
	}

	private string NamePassFile(string n1, string n2)
	{
		string text = "";
		int num = 0;
		do
		{
			if (num == 1 || num == 5)
			{
				text += NumToStr(Conversions.ToString(n2[num]), Conversions.ToString(n1[num]));
			}
			else
			{
				switch (num)
				{
				case 0:
					text += Sum1(Conversions.ToString(n1[num]), Conversions.ToString(n2[num]));
					if (n2.Length > 9)
					{
						int num3;
						try
						{
							num3 = checked(Conversions.ToInteger(n2[8].ToString()) + Conversions.ToInteger(n2[9].ToString()));
						}
						catch (Exception ex3)
						{
							ProjectData.SetProjectError(ex3);
							Exception ex4 = ex3;
							num3 = 27;
							ProjectData.ClearProjectError();
						}
						text = Conversions.ToString(num3) + Conversions.ToString(n2[8]) + text + Conversions.ToString(n2[9]);
					}
					break;
				case 3:
					text += NumToStr(Conversions.ToString(n1[num]), Conversions.ToString(n2[num]));
					if (n1.Length > 9)
					{
						int num2;
						try
						{
							num2 = checked(Conversions.ToInteger(n1[8].ToString()) * Conversions.ToInteger(n1[9].ToString()));
						}
						catch (Exception ex)
						{
							ProjectData.SetProjectError(ex);
							Exception ex2 = ex;
							num2 = 18;
							ProjectData.ClearProjectError();
						}
						text = Conversions.ToString(n1[9]) + text + Conversions.ToString(n1[8]) + Conversions.ToString(num2);
					}
					break;
				default:
					text = ((!(num == 2 || num == 7)) ? (text + Conversions.ToString(n1[num]) + Conversions.ToString(n2[num])) : (text + Sum(Conversions.ToString(n1[num]), Conversions.ToString(n2[num]), n2.Length)));
					break;
				}
			}
			num = checked(num + 1);
		}
		while (num <= 7);
		return text;
	}

	private string Sum2(string n1)
	{
		checked
		{
			string result;
			try
			{
				int num = 0;
				int num2 = default(int);
				do
				{
					num2 = ((!unchecked(num == 2 || num == 5)) ? (num2 + Conversions.ToInteger(n1[num].ToString()) * num) : (num2 * (Conversions.ToInteger(n1[num].ToString()) + num)));
					num++;
				}
				while (num <= 7);
				result = num2.ToString();
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

	private string Sum1(string n1, string n2)
	{
		string result;
		try
		{
			result = checked(Conversions.ToInteger(n1) * Conversions.ToInteger(n2) * Conversions.ToInteger(n1)).ToString();
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

	private string Sum(string n1, string n2, int l)
	{
		string result;
		try
		{
			result = checked((Conversions.ToInteger(n1) == Conversions.ToInteger(n2)) ? (Conversions.ToInteger(n1) * Conversions.ToInteger(n2) + l) : (Conversions.ToInteger(n1) * Conversions.ToInteger(n2) + Conversions.ToInteger(n2))).ToString();
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

	private string NumToStr(string n1, string n2)
	{
		string text = n1 switch
		{
			"0" => "a", 
			"1" => "d", 
			"2" => "om", 
			"3" => "u", 
			"4" => "c", 
			"5" => "w", 
			"6" => "ua", 
			"7" => "e", 
			"8" => "b", 
			"9" => "xy", 
			_ => "&", 
		};
		string text2 = n2 switch
		{
			"0" => "s", 
			"1" => "g", 
			"2" => "h", 
			"3" => "j", 
			"4" => "kq", 
			"5" => "l", 
			"6" => "v", 
			"7" => "n", 
			"8" => "r", 
			"9" => "f", 
			_ => "_", 
		};
		try
		{
			if (Conversions.ToInteger(n2) > 4)
			{
				text2 = text2.ToUpper();
			}
			if (Conversions.ToInteger(n1) > 4)
			{
				text = text.ToUpper();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		return text + text2;
	}
}
