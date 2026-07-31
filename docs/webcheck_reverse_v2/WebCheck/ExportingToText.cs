using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class ExportingToText : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("CBY")]
	private ComboBox _CBY;

	[CompilerGenerated]
	[AccessedThroughProperty("ExportB")]
	private Button _ExportB;

	[CompilerGenerated]
	[AccessedThroughProperty("BDir")]
	private Button _BDir;

	private string eFN;

	private ExportTextToFile ETF;

	internal virtual ComboBox CBY
	{
		[CompilerGenerated]
		get
		{
			return _CBY;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CBY_SelectedIndexChanged;
			ComboBox cBY = _CBY;
			if (cBY != null)
			{
				cBY.SelectedIndexChanged -= value2;
			}
			_CBY = value;
			cBY = _CBY;
			if (cBY != null)
			{
				cBY.SelectedIndexChanged += value2;
			}
		}
	}

	[field: AccessedThroughProperty("TextBoxS")]
	internal virtual TextBox TextBoxS
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ExportB
	{
		[CompilerGenerated]
		get
		{
			return _ExportB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ExportB_Click;
			Button exportB = _ExportB;
			if (exportB != null)
			{
				exportB.Click -= value2;
			}
			_ExportB = value;
			exportB = _ExportB;
			if (exportB != null)
			{
				exportB.Click += value2;
			}
		}
	}

	internal virtual Button BDir
	{
		[CompilerGenerated]
		get
		{
			return _BDir;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = BDir_Click;
			Button bDir = _BDir;
			if (bDir != null)
			{
				bDir.Click -= value2;
			}
			_BDir = value;
			bDir = _BDir;
			if (bDir != null)
			{
				bDir.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.ExportingToText));
		this.CBY = new System.Windows.Forms.ComboBox();
		this.TextBoxS = new System.Windows.Forms.TextBox();
		this.ExportB = new System.Windows.Forms.Button();
		this.BDir = new System.Windows.Forms.Button();
		this.Label1 = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.CBY.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.CBY.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CBY.FormattingEnabled = true;
		this.CBY.Location = new System.Drawing.Point(12, 12);
		this.CBY.Name = "CBY";
		this.CBY.Size = new System.Drawing.Size(170, 33);
		this.CBY.TabIndex = 2;
		this.TextBoxS.BackColor = System.Drawing.SystemColors.Window;
		this.TextBoxS.Enabled = false;
		this.TextBoxS.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TextBoxS.Location = new System.Drawing.Point(12, 62);
		this.TextBoxS.Multiline = true;
		this.TextBoxS.Name = "TextBoxS";
		this.TextBoxS.ReadOnly = true;
		this.TextBoxS.Size = new System.Drawing.Size(598, 121);
		this.TextBoxS.TabIndex = 12;
		this.TextBoxS.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.ExportB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ExportB.Location = new System.Drawing.Point(12, 199);
		this.ExportB.Name = "ExportB";
		this.ExportB.Size = new System.Drawing.Size(598, 47);
		this.ExportB.TabIndex = 14;
		this.ExportB.Text = "Почати експорт";
		this.ExportB.UseVisualStyleBackColor = true;
		this.BDir.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.BDir.Location = new System.Drawing.Point(188, 94);
		this.BDir.Name = "BDir";
		this.BDir.Size = new System.Drawing.Size(410, 47);
		this.BDir.TabIndex = 15;
		this.BDir.Text = "Відкрити папку з файлами";
		this.BDir.UseVisualStyleBackColor = true;
		this.BDir.Visible = false;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(268, 9);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(284, 48);
		this.Label1.TabIndex = 16;
		this.Label1.Text = "Чеки експортуються до папки:\r\n     Мої документи\\WebCheck";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(622, 263);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.BDir);
		base.Controls.Add(this.ExportB);
		base.Controls.Add(this.TextBoxS);
		base.Controls.Add(this.CBY);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "ExportingToText";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Експорт чеків за період";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public ExportingToText(string eF)
	{
		base.Load += ExportingToText_Load;
		ETF = new ExportTextToFile();
		InitializeComponent();
		eFN = eF;
	}

	private void ExportingToText_Load(object sender, EventArgs e)
	{
		int year = DateTime.Now.Year;
		for (int i = year; i >= 2021; i = checked(i + -1))
		{
			CBY.Items.Add(i.ToString());
		}
		CBY.Text = year.ToString();
		TextBoxS.Text = "Експорт чеків ПРРО ФН " + eFN + " за " + year + " рік";
	}

	private void CBY_SelectedIndexChanged(object sender, EventArgs e)
	{
		TextBoxS.Text = "Експорт чеків ПРРО ФН " + eFN + " за " + CBY.SelectedItem.ToString() + " рік";
	}

	private void ExportB_Click(object sender, EventArgs e)
	{
		int num = 0;
		ExportB.Enabled = false;
		CBY.Enabled = false;
		DirNewArh();
		ShiftAll shiftAll = new ShiftAll(CBY.SelectedItem.ToString());
		string text = "";
		checked
		{
			if (shiftAll.ShiftsYear > 0)
			{
				int shiftsYear = shiftAll.ShiftsYear;
				for (int i = 1; i <= shiftsYear; i++)
				{
					string expression = "-" + eFN + "-" + Strings.Replace(shiftAll.get_Seller(1, i), ".", "_");
					expression = Strings.Replace(expression, " ", "_");
					expression = Strings.Replace(expression, ":", "_");
					text = "\\" + shiftAll.get_Seller(0, i) + expression + ".txt";
					ETF.PathFile = MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString() + text;
					string text2 = ETF.NewFile();
					if (Operators.CompareString(text2, "", TextCompare: false) != 0)
					{
						TextBoxS.Text = "Помилка видалення файлу " + text2;
						ExportB.Enabled = true;
						CBY.Enabled = true;
						return;
					}
					CheckShift checkShift = new CheckShift(shiftAll.get_Seller(0, i));
					int checksShift = checkShift.ChecksShift;
					for (int j = 1; j <= checksShift; j++)
					{
						if (j != checkShift.ChecksShift)
						{
							TextBoxS.Text = " Eкспорт зміни № " + shiftAll.get_Seller(0, i) + "       чек: " + checkShift.get_Seller(0, j) + Environment.NewLine + " Залишилось змін: " + (1 + shiftAll.ShiftsYear - i);
						}
						else
						{
							TextBoxS.Text = " Eкспорт зміни № " + shiftAll.get_Seller(0, i) + "       чек: " + checkShift.get_Seller(0, j) + Environment.NewLine + " Залишилось змін: " + (shiftAll.ShiftsYear - i);
						}
						SaveChecktToFile(checkShift.get_Seller(0, j));
						num++;
						Application.DoEvents();
					}
				}
				TextBox textBoxS;
				(textBoxS = TextBoxS).Text = textBoxS.Text + Environment.NewLine + "---------" + Environment.NewLine + " ЕКСПОРТ ВИКОНАНО!!!   Опрацьовано чеків: " + num;
			}
			ExportB.Enabled = true;
			CBY.Enabled = true;
		}
	}

	private string MyDocPath()
	{
		return Environment.GetFolderPath(Environment.SpecialFolder.Personal);
	}

	private string SaveChecktToFile(string eN)
	{
		All.A.ExportLength = 30;
		CheckStamp checkStamp = new CheckStamp();
		if (checkStamp.CheckXML(eN).Length == 0)
		{
			ETF.NewFile();
			return "Помилка отримання чека: " + eN;
		}
		checked
		{
			int num = checkStamp.SizeCheck() - 1;
			for (int i = 0; i <= num; i++)
			{
				object obj = ETF.SaveTextToFile(checkStamp.LineFromCheck(i));
				if (Operators.ConditionalCompareObjectNotEqual(obj, "", TextCompare: false))
				{
					ETF.NewFile();
					return Conversions.ToString(Operators.ConcatenateObject("Помилка запису у файл ", obj));
				}
			}
			ETF.SaveTextToFile("");
			ETF.SaveTextToFile("");
			ETF.SaveTextToFile("");
			return "";
		}
	}

	private void DirNewArh()
	{
		if (!Directory.Exists(MyDocPath() + "\\WebCheck\\"))
		{
			Directory.CreateDirectory(MyDocPath() + "\\WebCheck\\");
		}
		if (!Directory.Exists(MyDocPath() + "\\WebCheck\\" + eFN + "\\"))
		{
			Directory.CreateDirectory(MyDocPath() + "\\WebCheck\\" + eFN + "\\");
		}
		if (!Directory.Exists(MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString() + "\\"))
		{
			Directory.CreateDirectory(MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString() + "\\");
		}
	}

	private void BDir_Click(object sender, EventArgs e)
	{
		DirNewArh();
		Interaction.Shell("explorer.exe " + MyDocPath() + "\\WebCheck\\" + eFN + "\\" + CBY.SelectedItem.ToString());
	}
}
