using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Dynamic;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Newtonsoft.Json;

namespace WebCheck;

[DesignerGenerated]
internal class FormImport : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ListBox1")]
	private CheckedListBox _ListBox1;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("SeaB")]
	private Button _SeaB;

	private int InfaC;

	private TypNumFiscal[] Infa;

	private string JS;

	[field: AccessedThroughProperty("TextBox1")]
	internal virtual TextBox TextBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckedListBox ListBox1
	{
		[CompilerGenerated]
		get
		{
			return _ListBox1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ListBox1_SelectedIndexChanged;
			CheckedListBox listBox = _ListBox1;
			if (listBox != null)
			{
				((ListBox)listBox).SelectedIndexChanged -= eventHandler;
			}
			_ListBox1 = value;
			listBox = _ListBox1;
			if (listBox != null)
			{
				((ListBox)listBox).SelectedIndexChanged += eventHandler;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
			}
		}
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("SeaT")]
	internal virtual TextBox SeaT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SeaB
	{
		[CompilerGenerated]
		get
		{
			return _SeaB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SeaB_Click;
			Button seaB = _SeaB;
			if (seaB != null)
			{
				((Control)seaB).Click -= eventHandler;
			}
			_SeaB = value;
			seaB = _SeaB;
			if (seaB != null)
			{
				((Control)seaB).Click += eventHandler;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0088: Unknown result type (might be due to invalid IL or missing references)
		//IL_0092: Expected O, but got Unknown
		//IL_010c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0116: Expected O, but got Unknown
		//IL_0190: Unknown result type (might be due to invalid IL or missing references)
		//IL_019a: Expected O, but got Unknown
		//IL_0226: Unknown result type (might be due to invalid IL or missing references)
		//IL_0230: Expected O, but got Unknown
		//IL_02ad: Unknown result type (might be due to invalid IL or missing references)
		//IL_02b7: Expected O, but got Unknown
		//IL_0322: Unknown result type (might be due to invalid IL or missing references)
		//IL_032c: Expected O, but got Unknown
		//IL_0436: Unknown result type (might be due to invalid IL or missing references)
		//IL_0440: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormImport));
		TextBox1 = new TextBox();
		ListBox1 = new CheckedListBox();
		OkB = new Button();
		NoB = new Button();
		SeaT = new TextBox();
		SeaB = new Button();
		((Control)this).SuspendLayout();
		((Control)TextBox1).Anchor = (AnchorStyles)9;
		((Control)TextBox1).Enabled = false;
		((Control)TextBox1).Font = new Font("Microsoft Sans Serif", 9f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TextBox1).Location = new Point(799, 12);
		((Control)TextBox1).Name = "TextBox1";
		((Control)TextBox1).Size = new Size(230, 24);
		((Control)TextBox1).TabIndex = 0;
		TextBox1.TextAlign = (HorizontalAlignment)2;
		((Control)ListBox1).Anchor = (AnchorStyles)15;
		((ListBox)ListBox1).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)ListBox1).FormattingEnabled = true;
		((Control)ListBox1).Location = new Point(12, 85);
		((Control)ListBox1).Name = "ListBox1";
		((Control)ListBox1).Size = new Size(1017, 378);
		((Control)ListBox1).TabIndex = 1;
		((Control)OkB).Anchor = (AnchorStyles)10;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(862, 469);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(167, 40);
		((Control)OkB).TabIndex = 7;
		((ButtonBase)OkB).Text = "Вибрати";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((Control)NoB).Anchor = (AnchorStyles)6;
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(12, 469);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(167, 40);
		((Control)NoB).TabIndex = 8;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)SeaT).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SeaT).Location = new Point(12, 32);
		((Control)SeaT).Name = "SeaT";
		((Control)SeaT).Size = new Size(414, 28);
		((Control)SeaT).TabIndex = 9;
		SeaT.TextAlign = (HorizontalAlignment)2;
		((Control)SeaB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SeaB).Location = new Point(447, 20);
		((Control)SeaB).Name = "SeaB";
		((Control)SeaB).Size = new Size(167, 40);
		((Control)SeaB).TabIndex = 10;
		((ButtonBase)SeaB).Text = "Пошук ";
		((ButtonBase)SeaB).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1042, 525);
		((Control)this).Controls.Add((Control)(object)SeaB);
		((Control)this).Controls.Add((Control)(object)SeaT);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)ListBox1);
		((Control)this).Controls.Add((Control)(object)TextBox1);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormImport";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Iмпорт даних ";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormImport(string jsonText)
	{
		((Form)this).Load += FormImport_Load;
		InfaC = 0;
		Infa = new TypNumFiscal[checked(InfaC + 1)];
		JS = "";
		InitializeComponent();
		JS = jsonText;
	}

	private void FormImport_Load(object sender, EventArgs e)
	{
		All.InfaImport.IPN = "";
		All.InfaImport.Name = "";
		All.InfaImport.Address = "";
		All.InfaImport.NumFiscal = "";
		All.InfaImport.TIN = "";
		All.InfaImport.OrgName = "";
		((Control)this).Show();
		Application.DoEvents();
		Dereban();
	}

	private bool Dereban()
	{
		((ObjectCollection)ListBox1.Items).Clear();
		((Control)SeaB).Enabled = false;
		((Control)OkB).Enabled = false;
		((Control)NoB).Enabled = false;
		checked
		{
			bool result;
			try
			{
				string[] array = JS.Split(new char[1] { '{' });
				int num = array.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					array[i] = Strings.Replace(array[i], "{", "", 1, -1, (CompareMethod)0);
					array[i] = Strings.Replace(array[i], "}", "", 1, -1, (CompareMethod)0);
					array[i] = Strings.Replace(array[i], "[", "\"WebCheck\"", 1, -1, (CompareMethod)0);
					array[i] = Strings.Replace(array[i], "],", "H*G*B", 1, -1, (CompareMethod)0);
					array[i] = Strings.Replace(array[i], "]", "", 1, -1, (CompareMethod)0);
				}
				TypEntity typEntity = default(TypEntity);
				typEntity.TIN = "";
				typEntity.OrgName = "";
				typEntity.Address = "";
				typEntity.IPN = "";
				typEntity.NameG = "";
				int num2 = 0;
				Infa = new TypNumFiscal[num2 + 1];
				int num3 = array.Length - 1;
				for (int i = 2; i <= num3; i++)
				{
					if (Operators.CompareString(KeyJ(array[i], "Entity"), "", false) != 0)
					{
						typEntity.OrgName = KeyJ(array[i], "OrgName");
						typEntity.Address = KeyJ(array[i], "Address");
						typEntity.IPN = KeyJ(array[i], "Ipn");
						typEntity.TIN = KeyJ(array[i], "Tin");
						typEntity.NameG = KeyJ(array[i], "Name");
					}
					else if (Operators.CompareString(KeyJ(array[i], "Closed", fn: true).ToLower(), "false", false) == 0)
					{
						Infa[num2].OrgName = typEntity.OrgName;
						Infa[num2].Address = typEntity.Address;
						Infa[num2].IPN = typEntity.IPN;
						Infa[num2].TIN = typEntity.TIN;
						Infa[num2].NumFiscal = KeyJ(array[i], "NumFiscal", fn: true);
						Infa[num2].Name = typEntity.NameG;
						((ObjectCollection)ListBox1.Items).Add((object)(Infa[num2].NumFiscal + "     " + Infa[num2].Address));
						num2++;
						ref TypNumFiscal[] infa = ref Infa;
						infa = (TypNumFiscal[])Utils.CopyArray((Array)infa, (Array)new TypNumFiscal[num2 + 1]);
					}
					TextBox1.Text = (array.Length - i - 1).ToString();
					Application.DoEvents();
				}
				TextBox1.Text = num2.ToString();
				((Control)SeaB).Enabled = true;
				((Control)OkB).Enabled = true;
				((Control)NoB).Enabled = true;
				result = true;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				TextBox1.Text = "Виникла помилка";
				((Control)SeaB).Enabled = true;
				((Control)OkB).Enabled = true;
				((Control)NoB).Enabled = true;
				result = false;
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private string KeyJ(string strJS, string Key, bool fn = false)
	{
		string result;
		try
		{
			if (!fn)
			{
				strJS = "{" + strJS.Trim() + "}";
			}
			else
			{
				strJS = "{" + strJS.Replace("e,", "e").Trim() + "}";
				strJS = Strings.Replace(strJS, "H*G*B", ",", 1, -1, (CompareMethod)0);
			}
			result = ((IDictionary<string, object>)JsonConvert.DeserializeObject<ExpandoObject>(strJS))[Key.Trim()].ToString();
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

	private void ListBox1_SelectedIndexChanged(object sender, EventArgs e)
	{
		checked
		{
			if (((ListBox)ListBox1).SelectedIndex >= 0)
			{
				int num = ((ObjectCollection)ListBox1.Items).Count - 1;
				for (int i = 0; i <= num; i++)
				{
					ListBox1.SetItemChecked(i, false);
				}
				ListBox1.SetItemChecked(((ListBox)ListBox1).SelectedIndex, true);
			}
		}
	}

	private void SeaB_Click(object sender, EventArgs e)
	{
		((Control)SeaB).Enabled = false;
		((Control)OkB).Enabled = false;
		((Control)NoB).Enabled = false;
		if (Operators.CompareString(SeaT.Text.Trim(), "", false) == 0)
		{
			SeashAll();
		}
		else
		{
			Seash();
		}
		((Control)SeaB).Enabled = true;
		((Control)OkB).Enabled = true;
		((Control)NoB).Enabled = true;
	}

	private void Seash()
	{
		((ObjectCollection)ListBox1.Items).Clear();
		InfaC = 0;
		string value = SeaT.Text.Trim();
		checked
		{
			int num = Infa.Length - 2;
			for (int i = 0; i <= num; i++)
			{
				if (Infa[i].NumFiscal.IndexOf(value) > -1)
				{
					((ObjectCollection)ListBox1.Items).Add((object)(Infa[i].NumFiscal + "     " + Infa[i].Address));
					InfaC++;
				}
				else if (Infa[i].Address.IndexOf(value) > -1)
				{
					((ObjectCollection)ListBox1.Items).Add((object)(Infa[i].NumFiscal + "     " + Infa[i].Address));
					InfaC++;
				}
			}
			TextBox1.Text = InfaC.ToString();
		}
	}

	private void SeashAll()
	{
		((ObjectCollection)ListBox1.Items).Clear();
		InfaC = 0;
		SeaT.Text.Trim();
		checked
		{
			int num = Infa.Length - 2;
			for (int i = 0; i <= num; i++)
			{
				((ObjectCollection)ListBox1.Items).Add((object)(Infa[i].NumFiscal + "     " + Infa[i].Address));
				InfaC++;
			}
			TextBox1.Text = InfaC.ToString();
		}
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		int selectedIndex = ((ListBox)ListBox1).SelectedIndex;
		if (selectedIndex > -1 && SeashImport(((ObjectCollection)ListBox1.Items)[selectedIndex].ToString()))
		{
			((Form)this).Close();
		}
	}

	private bool SeashImport(string sT)
	{
		sT = sT.Trim();
		string text = Conversions.ToString(sT[0]) + Conversions.ToString(sT[1]) + Conversions.ToString(sT[2]) + Conversions.ToString(sT[3]) + Conversions.ToString(sT[4]) + Conversions.ToString(sT[5]) + Conversions.ToString(sT[6]) + Conversions.ToString(sT[7]) + Conversions.ToString(sT[8]) + Conversions.ToString(sT[9]);
		checked
		{
			int num = Infa.Length - 2;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(Infa[i].NumFiscal, text, false) == 0)
				{
					All.InfaImport.IPN = Infa[i].IPN;
					All.InfaImport.Name = Infa[i].Name;
					All.InfaImport.Address = Infa[i].Address;
					All.InfaImport.NumFiscal = Infa[i].NumFiscal;
					All.InfaImport.TIN = Infa[i].TIN;
					All.InfaImport.OrgName = Infa[i].OrgName;
					return true;
				}
			}
			return false;
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		All.InfaImport.IPN = "";
		All.InfaImport.Name = "";
		All.InfaImport.Address = "";
		All.InfaImport.NumFiscal = "";
		All.InfaImport.TIN = "";
		All.InfaImport.OrgName = "";
		((Form)this).Close();
	}
}
